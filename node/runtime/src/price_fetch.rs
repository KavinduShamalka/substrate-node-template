/// A runtime module template with necessary imports

/// Feel free to remove or edit this file as needed.
/// If you change the name of this file, make sure to update its references in runtime/src/lib.rs
/// If you remove this file, you can remove those references


/// For more guidance on Substrate modules, see the example module
/// https://github.com/paritytech/substrate/blob/master/srml/example/src/lib.rs

// We have to import a few things
use rstd::prelude::*;
use app_crypto::RuntimeAppPublic;
use support::{decl_module, decl_storage, decl_event, print, dispatch::Result};
use system::ensure_signed;
use system::offchain::SubmitSignedTransaction;
use codec::{Encode, Decode};
// #[cfg(feature = "std")]
use simple_json::{self, json::JsonValue, parser::Parser};

#[derive(Encode, Decode, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct Price {
	dollars: u32,
	cents: u32, // up to 4 digits
	currency: Vec<u8>,
}

impl Price {
	fn new(dollars: u32, cents: u32, currency: Option<Vec<u8>>) -> Price {
		match currency {
			Some(curr) => Price{dollars, cents, currency: curr},
			None => Price{dollars, cents, currency: b"usd".to_vec()}
		}
	}
}

/// Our local KeyType.
///
/// For security reasons the offchain worker doesn't have direct access to the keys
/// but only to app-specific subkeys, which are defined and grouped by their `KeyTypeId`.
/// We define it here as `ofcb` (for `offchain callback`). Yours should be specific to
/// the module you are actually building.
pub const KEY_TYPE: app_crypto::KeyTypeId = app_crypto::KeyTypeId(*b"ofpf");

pub const FETCHED_CRYPTOS: [(&'static [u8], &'static [u8], &'static [u8]); 2] = [
	(b"BTC", b"coincap",
		b"https://api.coincap.io/v2/assets/bitcoin"),
	(b"BTC", b"coinmarketcap",
		b"https://sandbox-api.coinmarketcap.com/v1/cryptocurrency/quotes/latest?CMC_PRO_API_KEY=2e6d8847-bcea-4999-87b1-ad452efe4e40&symbol=BTC"),
	// (b"ETH", b"coincap",
	// 	b"https://api.coincap.io/v2/assets/ethereum"),
	// (b"ETH", b"coinmarketcap",
	// 	b"https://sandbox-api.coinmarketcap.com/v1/cryptocurrency/quotes/latest?CMC_PRO_API_KEY=2e6d8847-bcea-4999-87b1-ad452efe4e40&symbol=ETH"),
];

/// The module's configuration trait.
pub trait Trait: timestamp::Trait + system::Trait {
	/// The overarching event type.
	type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;

	/// A dispatchable call type. We need to define it for the offchain worker to
	/// reference the `pong` function it wants to call.
	type Call: From<Call<Self>>;

	/// Let's define the helper we use to create signed transactions with
	type SubmitTransaction: SubmitSignedTransaction<Self, <Self as Trait>::Call>;

	/// The local keytype
	type KeyType: RuntimeAppPublic + From<Self::AccountId> + Into<Self::AccountId> + Clone;
}

/// The type of requests we can send to the offchain worker
#[cfg_attr(feature = "std", derive(PartialEq, Debug))]
#[derive(Encode, Decode)]
pub enum OffchainRequest<T: system::Trait> {
	/// If an authorised offchain worker sees this, will kick off to work
	PriceFetch(<T as system::Trait>::AccountId, (Vec<u8>, Vec<u8>, Vec<u8>))
}

decl_event!(
	pub enum Event<T> where
		Moment = <T as timestamp::Trait>::Moment {
		PriceFetched(Vec<u8>, Vec<u8>, Moment, Option<Price>),
	}
);

// This module's storage items.
decl_storage! {
	trait Store for Module<T: Trait> as PriceFetch {
		pub OcRequests get(oc_requests): Vec<OffchainRequest<T>>;
		pub PricePoints: map (Vec<u8>, Vec<u8>) => Vec<(T::Moment, Option<Price>)>;
	}
}

// The module's dispatchable functions.
decl_module! {
	/// The module declaration.
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		// Initializing events
		// this is needed only if you are using events in your module
		fn deposit_event() = default;

		// Clean the state on initialization of the block
		fn on_initialize(_block: T::BlockNumber) {
			<Self as Store>::OcRequests::kill();
		}

		pub fn kickoff_pricefetch(origin) -> Result {
			let who = ensure_signed(origin)?;

			for cyrpto_info in FETCHED_CRYPTOS.iter() {
				<Self as Store>::OcRequests::mutate(|v|
					v.push(OffchainRequest::PriceFetch(
						who.clone(),
						(cyrpto_info.0.to_vec(), cyrpto_info.1.to_vec(), cyrpto_info.2.to_vec())
					))
				);
			}
			Ok(())
		}

		pub fn record_price(origin, crypto_info: (Vec<u8>, Vec<u8>, Vec<u8>), price: Option<Price>) -> Result {
			runtime_io::print_utf8(b"record_price: called");

			// TODO: add mechanism to check origin has to be trusted (session key, etc)
			let sender = ensure_signed(origin)?;

			let (symbol, source) = (crypto_info.0, crypto_info.1);
			let now = <timestamp::Module<T>>::get();

			// Spit out an event and Add to storage
			Self::deposit_event(RawEvent::PriceFetched(
				symbol.clone(), source.clone(), now.clone(), price.clone()));
			let price_pt = (now, price);
			<PricePoints<T>>::mutate((symbol, source), |vec| vec.push(price_pt));
			Ok(())
		}

		fn offchain_worker(_block: T::BlockNumber) {
			// #[cfg(feature = "std")]
			for fetch_info in Self::oc_requests() {
				// enhancement: batch the fetches together and send an array to
				//   `http_response_wait` in one go.
				let _ = match fetch_info {
					OffchainRequest::PriceFetch(who, crypto_info) => Self::fetch_price(who, crypto_info)
				};
			}
		} // end of `fn offchain_worker`
	}
}

impl<T: Trait> Module<T> {
	// #[cfg(feature = "std")]
	fn fetch_price(key: T::AccountId, crypto_info: (Vec<u8>, Vec<u8>, Vec<u8>)) -> Result {
		runtime_io::print_utf8(&crypto_info.0);
		runtime_io::print_utf8(&crypto_info.1);
		runtime_io::print_utf8(&crypto_info.2);
		runtime_io::print_utf8(b"---");
		let remote_url: &str = rstd::str::from_utf8(&crypto_info.2).unwrap();
		let id = runtime_io::http_request_start("GET", remote_url, &[])
			.map_err(|_| "http_request start error")?;
		let _status = runtime_io::http_response_wait(&[id], None);

		let mut json_result: Vec<u8> = vec![];
		loop {
			let mut buffer = vec![0; 1024];
			let _read = runtime_io::http_response_read_body(id, &mut buffer, None).map_err(|_e| ());
			json_result = [&json_result[..], &buffer[..]].concat();
			if _read == Ok(0) { break }
		}

		// Print the whole JSON blob
		runtime_io::print_utf8(&json_result);

		let json_obj: JsonValue = simple_json::parse_json(
			&rstd::str::from_utf8(&json_result).unwrap()).unwrap();

		let price = match crypto_info.1.as_slice() {
			src if src == b"coincap" => Self::fetch_price_from_coincap(json_obj),
		  src if src == b"coinmarketcap" => Self::fetch_price_from_coinmarketcap(json_obj),
		  _ => panic!("unknown source: {:?}", crypto_info.1),
		};

		let call = Call::record_price(crypto_info, price);
		T::SubmitTransaction::sign_and_submit(call, key.clone().into())
			.map_err(|_| {
				print("fetch_price_signing error");
				"fetch_price_signing error"
			})
	}

	// #[cfg(feature = "std")]
	fn fetch_price_from_coincap(json: JsonValue) -> Option<Price> {
		runtime_io::print_utf8(b"-- fetch_price_from_coincap");
		Some(Price::new(88, 3205, None))
	}

	// #[cfg(feature = "std")]
	fn fetch_price_from_coinmarketcap(json: JsonValue) -> Option<Price> {
		runtime_io::print_utf8(b"-- fetch_price_from_coinmarketcap");
		Some(Price::new(103, 3205, None))
	}
}

/// tests for this module
#[cfg(test)]
mod tests {
	use super::*;

	use primitives::H256;
	use support::{impl_outer_origin, assert_ok, parameter_types};
	use sr_primitives::{
		traits::{BlakeTwo256, IdentityLookup}, testing::Header, weights::Weight, Perbill,
	};

	impl_outer_origin! {
		pub enum Origin for Test {}
	}

	// For testing the module, we construct most of a mock runtime. This means
	// first constructing a configuration type (`Test`) which `impl`s each of the
	// configuration traits of modules we want to use.
	#[derive(Clone, Eq, PartialEq)]
	pub struct Test;
	parameter_types! {
		pub const BlockHashCount: u64 = 250;
		pub const MaximumBlockWeight: Weight = 1024;
		pub const MaximumBlockLength: u32 = 2 * 1024;
		pub const AvailableBlockRatio: Perbill = Perbill::from_percent(75);
	}
	impl system::Trait for Test {
		type Origin = Origin;
		type Call = ();
		type Index = u64;
		type BlockNumber = u64;
		type Hash = H256;
		type Hashing = BlakeTwo256;
		type AccountId = u64;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Header = Header;
		type Event = ();
		type BlockHashCount = BlockHashCount;
		type MaximumBlockWeight = MaximumBlockWeight;
		type MaximumBlockLength = MaximumBlockLength;
		type AvailableBlockRatio = AvailableBlockRatio;
		type Version = ();
	}
	impl Trait for Test {
		type Event = ();
	}
	type TemplateModule = Module<Test>;

	// This function basically just builds a genesis storage key/value store according to
	// our desired mockup.
	fn new_test_ext() -> runtime_io::TestExternalities {
		system::GenesisConfig::default().build_storage::<Test>().unwrap().into()
	}

	#[test]
	fn it_works_for_default_value() {
		new_test_ext().execute_with(|| {
			// Just a dummy test for the dummy funtion `do_something`
			// calling the `do_something` function with a value 42
			assert_ok!(TemplateModule::do_something(Origin::signed(1), 42));
			// asserting that the stored value is equal to what we stored
			assert_eq!(TemplateModule::something(), Some(42));
		});
	}
}
