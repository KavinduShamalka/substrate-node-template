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
use system::offchain::{SubmitSignedTransaction, SubmitUnsignedTransaction};
use codec::{Encode, Decode};
use simple_json::{ self, json::JsonValue };
use core::convert::{ TryInto };
use sr_primitives::{
  transaction_validity::{
    TransactionValidity, TransactionLongevity, ValidTransaction, InvalidTransaction
  }
};

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq)]
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

// This automates price fetching every certain blocks. Set to 0 disable this feature.
//   Then you need to manucally kickoff pricefetch
pub const BLOCK_FETCH_DUR: u64 = 5;

pub const FETCHED_CRYPTOS: [(&'static [u8], &'static [u8], &'static [u8]); 1] = [
	(b"BTC", b"coincap",
		b"https://api.coincap.io/v2/assets/bitcoin"),
	// (b"BTC", b"coinmarketcap",
	// 	b"https://sandbox-api.coinmarketcap.com/v1/cryptocurrency/quotes/latest?CMC_PRO_API_KEY=2e6d8847-bcea-4999-87b1-ad452efe4e40&symbol=BTC"),
	// (b"ETH", b"coincap",
	// 	b"https://api.coincap.io/v2/assets/ethereum"),
	// (b"ETH", b"coinmarketcap",
	// 	b"https://sandbox-api.coinmarketcap.com/v1/cryptocurrency/quotes/latest?CMC_PRO_API_KEY=2e6d8847-bcea-4999-87b1-ad452efe4e40&symbol=ETH"),
];

pub type StdResult<T> = core::result::Result<T, &'static str>;

/// The module's configuration trait.
pub trait Trait: timestamp::Trait + system::Trait {
	/// The overarching event type.
	type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;

	/// A dispatchable call type. We need to define it for the offchain worker to
	/// reference the `pong` function it wants to call.
	type Call: From<Call<Self>>;

	/// Let's define the helper we use to create signed transactions with
	type SubmitTransaction: SubmitSignedTransaction<Self, <Self as Trait>::Call>;
  type SubmitUnsignedTransaction: SubmitUnsignedTransaction<Self, <Self as Trait>::Call>;

	/// The local keytype
	type KeyType: RuntimeAppPublic + From<Self::AccountId> + Into<Self::AccountId> + Clone;
}

/// The type of requests we can send to the offchain worker
#[cfg_attr(feature = "std", derive(PartialEq, Debug))]
#[derive(Encode, Decode)]
pub enum OffchainRequest {
	/// If an authorised offchain worker sees this, will kick off to work
	PriceFetch(Vec<u8>, Vec<u8>, Vec<u8>)
}

decl_event!(
	pub enum Event<T> where
		Moment = <T as timestamp::Trait>::Moment {

		PriceFetched(Vec<u8>, Vec<u8>, Moment, Price),
	}
);

// This module's storage items.
decl_storage! {
	trait Store for Module<T: Trait> as PriceFetch {
		// storage about offchain worker tasks
		pub OcRequests get(oc_requests): Vec<OffchainRequest>;

		// storage about source price points
		pub SrcPricePoints get(src_price_pts): Vec<(T::Moment, Price)>;
		pub TokenSrcPPMap: map Vec<u8> => Vec<u64>;
		pub RemoteSrcPPMap: map Vec<u8> => Vec<u64>;

		// storage about aggregated price points (calculated in our logic)
		pub AggPricePoints get(agg_price_pts): Vec<(T::Moment, Price)>;
		pub AggPPDependencies: map Vec<u8> => Vec<u64>;
		pub TokenAggPPMap: map Vec<u8> => Vec<u64>;
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
		fn on_initialize(block: T::BlockNumber) {
			<Self as Store>::OcRequests::kill();

			if BLOCK_FETCH_DUR > 0 && (TryInto::<u64>::try_into(block).ok().unwrap()) % BLOCK_FETCH_DUR == 0 {
				let _ = Self::enque_pricefetch_tasks();
			}
		}

		pub fn kickoff_pricefetch(origin) -> Result {
			let _who = ensure_signed(origin)?;
			Self::enque_pricefetch_tasks()
		}

		pub fn record_price(_origin, crypto_info: (Vec<u8>, Vec<u8>, Vec<u8>), price: Price) -> Result {
			let (symbol, remote_src) = (crypto_info.0, crypto_info.1);
			let now = <timestamp::Module<T>>::get();

			// Debug printout
			runtime_io::print_utf8(b"record_price: called");
			runtime_io::print_utf8(&symbol);
			runtime_io::print_utf8(&remote_src);
			runtime_io::print_num(price.dollars.into());
			runtime_io::print_num(price.cents.into());

			// Spit out an event and Add to storage
			Self::deposit_event(RawEvent::PriceFetched(
				symbol.clone(), remote_src.clone(), now.clone(), price.clone()));

			let price_pt = (now, price);
			<SrcPricePoints<T>>::mutate(|vec| vec.push(price_pt));
			// The index serves as the ID
			let pp_id: u64 = Self::src_price_pts().len().try_into().unwrap();
			<TokenSrcPPMap>::mutate(symbol, |token_vec| token_vec.push(pp_id));
			<RemoteSrcPPMap>::mutate(remote_src, |rs_vec| rs_vec.push(pp_id));

			Ok(())
		}

		pub fn aggregate_price_points(_origin) -> Result {
			Ok(())
		}

		fn offchain_worker(_block: T::BlockNumber) {
			for fetch_info in Self::oc_requests() {
				// enhancement: batch the fetches together and send an array to
				//   `http_response_wait` in one go.
				let res = match fetch_info {
					OffchainRequest::PriceFetch(sym, remote_src, remote_url) =>
						Self::fetch_price(sym, remote_src, remote_url)
				};

				if let Err(err_msg) = res { print(err_msg) }
			}

			// submit a call back onchain to aggregate price
			let call = Call::aggregate_price_points();
			let _ = T::SubmitUnsignedTransaction::submit_unsigned(call)
				.map_err(|_| "aggregate_price_points: submit_unsigned_call error");

		} // end of `fn offchain_worker`

	}
}

impl<T: Trait> Module<T> {
	fn enque_pricefetch_tasks() -> Result {
		for crypto_info in FETCHED_CRYPTOS.iter() {
			<Self as Store>::OcRequests::mutate(|v|
				v.push(OffchainRequest::PriceFetch(crypto_info.0.to_vec(),
					crypto_info.1.to_vec(), crypto_info.2.to_vec()))
			);
		}
		Ok(())
	}

	fn fetch_json(remote_url: &str) -> StdResult<JsonValue> {
		let id = runtime_io::http_request_start("GET", remote_url, &[])
			.map_err(|_| "http_request start error")?;
		let _ = runtime_io::http_response_wait(&[id], None);

		let mut json_result: Vec<u8> = vec![];
		loop {
			let mut buffer = vec![0; 1024];
			let _read = runtime_io::http_response_read_body(id, &mut buffer, None).map_err(|_e| ());
			json_result = [&json_result[..], &buffer[..]].concat();
			if _read == Ok(0) { break }
		}

		// Print out the whole JSON blob
		runtime_io::print_utf8(&json_result);

		let json_val: JsonValue = simple_json::parse_json(
			&rstd::str::from_utf8(&json_result).unwrap())
			.map_err(|_| "JSON parsing error")?;

		Ok(json_val)
	}

	fn fetch_price(sym: Vec<u8>, remote_src: Vec<u8>, remote_url: Vec<u8>) -> Result {
		runtime_io::print_utf8(&sym);
		runtime_io::print_utf8(&remote_src);
		runtime_io::print_utf8(b"---");

		let json = Self::fetch_json(rstd::str::from_utf8(&remote_url).unwrap())?;

		let price = match remote_src.as_slice() {
			src if src == b"coincap" => Self::fetch_price_from_coincap(json)
				.map_err(|_| "fetch_price_from_coincap error")?,
		  src if src == b"coinmarketcap" => Self::fetch_price_from_coinmarketcap(json)
		  	.map_err(|_| "fetch_price_from_coinmarketcap error")?,
		  _ => return Err("Unknown remote source"),
		};

		let call = Call::record_price((sym, remote_src, remote_url), price);
		T::SubmitUnsignedTransaction::submit_unsigned(call)
			.map_err(|_| "fetch_price: submit_unsigned_call error")
	}

	fn fetch_price_from_coincap(_json: JsonValue) -> StdResult<Price> {
		runtime_io::print_utf8(b"-- fetch_price_from_coincap");
		Ok(Price::new(100, 3500, None))
	}

	fn fetch_price_from_coinmarketcap(_json: JsonValue) -> StdResult<Price> {
		runtime_io::print_utf8(b"-- fetch_price_from_coinmarketcap");
		Ok(Price::new(200, 5000, None))
	}
}

impl<T: Trait> support::unsigned::ValidateUnsigned for Module<T> {
  type Call = Call<T>;

  fn validate_unsigned(call: &Self::Call) -> TransactionValidity {
		let now = <timestamp::Module<T>>::get();
  	match call {
  		Call::record_price(crypto_info, price) =>	Ok(ValidTransaction {
        priority: 0,
        requires: vec![],
        provides: vec![(crypto_info, price).encode()],
        longevity: TransactionLongevity::max_value(),
        propagate: true,
  		}),
  		Call::aggregate_price_points() => Ok(ValidTransaction {
        priority: 0,
        requires: vec![],
        provides: vec![(now).encode()],
        longevity: TransactionLongevity::max_value(),
        propagate: true,
  		}),
  		_ => InvalidTransaction::Call.into()
  	}
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
