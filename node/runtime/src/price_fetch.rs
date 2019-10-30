/// A runtime module template with necessary imports

/// Feel free to remove or edit this file as needed.
/// If you change the name of this file, make sure to update its references in runtime/src/lib.rs
/// If you remove this file, you can remove those references


/// For more guidance on Substrate modules, see the example module
/// https://github.com/paritytech/substrate/blob/master/srml/example/src/lib.rs

// We have to import a few things
use rstd::prelude::*;
use app_crypto::RuntimeAppPublic;
use support::{decl_module, decl_storage, decl_event, dispatch::Result};
// use system::{ensure_signed, ensure_root};
use system::{ensure_signed};
use system::offchain::{SubmitSignedTransaction};
use codec::{Encode, Decode};

type StdResult<T> = core::result::Result<T, ()>;

/// Our local KeyType.
///
/// For security reasons the offchain worker doesn't have direct access to the keys
/// but only to app-specific subkeys, which are defined and grouped by their `KeyTypeId`.
/// We define it here as `ofcb` (for `offchain callback`). Yours should be specific to
/// the module you are actually building.
pub const KEY_TYPE: app_crypto::KeyTypeId = app_crypto::KeyTypeId(*b"ofpf");

pub const FETCHED_CRYPTOS: [(&'static [u8], &'static [u8], &'static [u8]); 4] = [
	(b"BTC", b"coincap",
		b"https://api.coincap.io/v2/assets/bitcoin"),
	(b"BTC", b"coinmarketcap",
		b"https://sandbox-api.coinmarketcap.com/v1/cryptocurrency/quotes/latest?CMC_PRO_API_KEY=2e6d8847-bcea-4999-87b1-ad452efe4e40&symbol=BTC"),
	(b"ETH", b"coincap",
		b"https://api.coincap.io/v2/assets/ethereum"),
	(b"ETH", b"coinmarketcap",
		b"https://sandbox-api.coinmarketcap.com/v1/cryptocurrency/quotes/latest?CMC_PRO_API_KEY=2e6d8847-bcea-4999-87b1-ad452efe4e40&symbol=ETH"),
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
	/// If an authorised offchain worker sees this ping, it shall respond with a `pong` call
	PriceFetch(<T as system::Trait>::AccountId, (Vec<u8>, Vec<u8>, Vec<u8>))
}

decl_event!(
	pub enum Event<T> where
		Moment = <T as timestamp::Trait>::Moment {

		PriceFetched(Vec<u8>, Moment, u32, u32),
	}
);

// This module's storage items.
decl_storage! {
	trait Store for Module<T: Trait> as PriceFetch {
		OcRequests get(oc_requests): Vec<OffchainRequest<T>>;

		// using a tuple struct, 1st value is dollar, 2nd value is cent
		Prices get(prices): map Vec<u8> => (u32, u32);
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
			// DEBUGGING
			runtime_io::print_utf8(b"on_initialize");
			<Self as Store>::OcRequests::kill();
		}

		// Just a dummy entry point.
		// function that can be called by the external world as an extrinsics call
		// takes a parameter of the type `AccountId`, stores it and emits an event
		pub fn kickoff_pricefetch(origin) -> Result {
			let who = ensure_signed(origin)?;

			// DEBUGGING
			runtime_io::print_utf8(b"kickoff_pricefetch");

			for fetch in FETCHED_CRYPTOS.iter() {
				<Self as Store>::OcRequests::mutate(|v|
					v.push(OffchainRequest::PriceFetch(who.clone(),
						(fetch.0.to_vec(), fetch.1.to_vec(), fetch.2.to_vec())
					))
				);
			}

			Ok(())
		}

		fn offchain_worker(_block: T::BlockNumber) {
			runtime_io::print_utf8(b"offchain_worker kick in");

			let fetches = Self::oc_requests();
			for fetch in fetches {
				if let OffchainRequest::PriceFetch(who, fetch_info) = fetch {
					// enhancement: group the fetch together and send an array to `http_response_wait` in one
					//   go.
					Self::fetch_price(who, fetch_info);
				}
			}
		}
	}
}

impl<T: Trait> Module<T> {
	fn fetch_price(_key: T::AccountId, fetch_info: (Vec<u8>, Vec<u8>, Vec<u8>)) -> StdResult<()> {
		runtime_io::print_utf8(&fetch_info.0);
		runtime_io::print_utf8(&fetch_info.1);
		runtime_io::print_utf8(&fetch_info.2);
		runtime_io::print_utf8(b"---");
		let id = runtime_io::http_request_start("GET",
			rstd::str::from_utf8(&fetch_info.2).unwrap(), &[])?;
		let _status = runtime_io::http_response_wait(&[id], None);
		let mut buffer = vec![0; 1024];
		let _read = runtime_io::http_response_read_body(id, &mut buffer, None).map_err(|_e| ());
		runtime_io::print_utf8(&buffer);

		Ok(())
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
