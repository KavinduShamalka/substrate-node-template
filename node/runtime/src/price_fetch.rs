/// A runtime module template with necessary imports

/// Feel free to remove or edit this file as needed.
/// If you change the name of this file, make sure to update its references in runtime/src/lib.rs
/// If you remove this file, you can remove those references

/// For more guidance on Substrate modules, see the example module
/// https://github.com/paritytech/substrate/blob/master/srml/example/src/lib.rs

// We have to import a few things
use rstd::{prelude::*, convert::TryInto};
use primitives::crypto::KeyTypeId;
use support::{decl_module, decl_storage, decl_event, dispatch, dispatch::DispatchError,
  debug, traits::Get};
use system::offchain::{SubmitSignedTransaction, SubmitUnsignedTransaction, SignAndSubmitTransaction};
use simple_json::{self, json::JsonValue};
use runtime_io::{self, misc::print_utf8 as print_bytes};
#[cfg(not(feature = "std"))]
use num_traits::float::FloatCore;
use codec::Encode;
use sp_runtime::{
  offchain::http,
  transaction_validity::{
    TransactionValidity, TransactionLongevity, ValidTransaction, InvalidTransaction
  }
};

type StdResult<T> = core::result::Result<T, &'static str>;

/// Our local KeyType.
///
/// For security reasons the offchain worker doesn't have direct access to the keys
/// but only to app-specific subkeys, which are defined and grouped by their `KeyTypeId`.
pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"ofpf");

// REVIEW-CHECK: is it necessary to wrap-around storage vector at `MAX_VEC_LEN`?
pub const MAX_VEC_LEN: usize = 1000;

pub mod crypto {
  pub use super::KEY_TYPE;
  use sp_runtime::app_crypto::{app_crypto, sr25519};
  app_crypto!(sr25519, KEY_TYPE);
}

pub const FETCHED_CRYPTOS: [(&[u8], &[u8], &[u8]); 6] = [
  (b"BTC", b"coincap",
    b"https://api.coincap.io/v2/assets/bitcoin"),
  (b"BTC", b"cryptocompare",
    b"https://min-api.cryptocompare.com/data/price?fsym=BTC&tsyms=USD"),
  (b"ETH", b"coincap",
   b"https://api.coincap.io/v2/assets/ethereum"),
  (b"ETH", b"cryptocompare",
    b"https://min-api.cryptocompare.com/data/price?fsym=ETH&tsyms=USD"),
  (b"DAI", b"coincap",
    b"https://api.coincap.io/v2/assets/dai"),
  (b"DAI", b"cryptocompare",
    b"https://min-api.cryptocompare.com/data/price?fsym=DAI&tsyms=USD"),
];

/// The module's configuration trait.
pub trait Trait: timestamp::Trait + system::Trait {
  /// The overarching event type.
  type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
  type Call: From<Call<Self>>;

  type SubmitSignedTransaction: SubmitSignedTransaction<Self, <Self as Trait>::Call>;
  type SignAndSubmitTransaction: SignAndSubmitTransaction<Self, <Self as Trait>::Call>;
  type SubmitUnsignedTransaction: SubmitUnsignedTransaction<Self, <Self as Trait>::Call>;

  // Wait period between automated fetches. Set to 0 disable this feature.
  //   Then you need to manucally kickoff pricefetch
  type BlockFetchDur: Get<Self::BlockNumber>;
}

decl_event!(
  pub enum Event<T> where
    Moment = <T as timestamp::Trait>::Moment {

    FetchedPrice(Vec<u8>, Vec<u8>, Moment, u64),
    AggregatedPrice(Vec<u8>, Moment, u64),
  }
);

// This module's storage items.
decl_storage! {
  trait Store for Module<T: Trait> as PriceFetch {
    UpdateAggPP get(update_agg_pp): linked_map Vec<u8> => u32 = 0;

    // storage about source price points
    // mapping of ind -> (timestamp, price)
    //   price has been inflated by 10,000, and in USD.
    //   When used, it should be divided by 10,000.
    SrcPricePoints get(src_price_pts): Vec<(T::Moment, u64)>;

    // mapping of token sym -> pp_ind
    // Using linked map for easy traversal from offchain worker or UI
    TokenSrcPPMap: linked_map Vec<u8> => Vec<u32>;

    // mapping of remote_src -> pp_ind
    RemoteSrcPPMap: linked_map Vec<u8> => Vec<u32>;

    // storage about aggregated price points (calculated with our logic)
    AggPricePoints get(agg_price_pts): Vec<(T::Moment, u64)>;
    TokenAggPPMap: linked_map Vec<u8> => Vec<u32>;
  }
}

// The module's dispatchable functions.
decl_module! {
  /// The module declaration.
  pub struct Module<T: Trait> for enum Call where origin: T::Origin {
    // Initializing events
    // this is needed only if you are using events in your module
    fn deposit_event() = default;

    pub fn record_price(
      _origin,
      _block: T::BlockNumber,
      crypto_info: (Vec<u8>, Vec<u8>, Vec<u8>),
      price: u64
    ) -> dispatch::DispatchResult {
      let (sym, remote_src) = (crypto_info.0, crypto_info.1);
      let now = <timestamp::Module<T>>::get();

      // Debug printout
      debug::info!("record_price: {:?}, {:?}, {:?}",
        core::str::from_utf8(&sym).unwrap(),
        core::str::from_utf8(&remote_src).unwrap(),
        price
      );

      // Spit out an event and Add to storage
      Self::deposit_event(RawEvent::FetchedPrice(
        sym.clone(), remote_src.clone(), now.clone(), price));

      let price_pt = (now, price);
      // The index serves as the ID
      let pp_id: u32 = Self::src_price_pts().len().try_into().unwrap();
      <SrcPricePoints<T>>::mutate(|vec| vec.push(price_pt));
      <TokenSrcPPMap>::mutate(sym.clone(), |token_vec| token_vec.push(pp_id));
      <RemoteSrcPPMap>::mutate(remote_src, |rs_vec| rs_vec.push(pp_id));

      // set the flag to kick off update aggregated pricing in offchain call
      <UpdateAggPP>::mutate(sym.clone(), |freq| *freq += 1);

      Ok(())
    }

    pub fn record_agg_pp(
      _origin,
      _block: T::BlockNumber,
      sym: Vec<u8>,
      price: u64
    ) -> dispatch::DispatchResult {
      // Debug printout
      debug::info!("record_agg_pp: {}: {:?}",
        core::str::from_utf8(&sym).unwrap(),
        price
      );

      let now = <timestamp::Module<T>>::get();

      // Spit the event
      Self::deposit_event(RawEvent::AggregatedPrice(
        sym.clone(), now.clone(), price.clone()));

      // Record in the storage
      let price_pt = (now.clone(), price.clone());
      let pp_id: u32 = Self::agg_price_pts().len().try_into().unwrap();
      <AggPricePoints<T>>::mutate(|vec| vec.push(price_pt));
      <TokenAggPPMap>::mutate(sym.clone(), |vec| vec.push(pp_id));

      // Turn off the flag as the request has been handled
      <UpdateAggPP>::mutate(sym.clone(), |freq| *freq = 0);

      Ok(())
    }

    fn offchain_worker(block: T::BlockNumber) {
      let duration = T::BlockFetchDur::get();

      // Type I task: fetch_price
      if duration > 0.into() && block % duration == 0.into() {
        for (sym, remote_src, remote_url) in FETCHED_CRYPTOS.iter() {
          if let Err(e) = Self::fetch_price(block, *sym, *remote_src, *remote_url) {
            debug::error!("Error fetching: {:?}, {:?}: {:?}",
              core::str::from_utf8(sym).unwrap(),
              core::str::from_utf8(remote_src).unwrap(),
              e);
          }
        }
      }

      // Type II task: aggregate price
      <UpdateAggPP>::enumerate()
        // filter those to be updated
        .filter(|(_, freq)| *freq > 0)
        .for_each(|(sym, freq)| {
          if let Err(e) = Self::aggregate_pp(block, &sym, freq as usize) {
            debug::error!("Error aggregating price of {:?}: {:?}",
              core::str::from_utf8(&sym).unwrap(), e);
          }
        });
    } // end of `fn offchain_worker()`

  }
}

impl<T: Trait> Module<T> {
  fn fetch_json<'a>(remote_url: &'a [u8]) -> StdResult<JsonValue> {
    let remote_url_str = core::str::from_utf8(remote_url)
      .map_err(|_| "Error in converting remote_url to string")?;

    let pending = http::Request::get(remote_url_str).send()
      .map_err(|_| "Error in sending http GET request")?;

    let response = pending.wait()
      .map_err(|_| "Error in waiting http response back")?;

    if response.code != 200 {
      debug::warn!("Unexpected status code: {}", response.code);
      return Err("Non-200 status code returned from http request");
    }

    let json_result: Vec<u8> = response.body().collect::<Vec<u8>>();

    // Print out the whole JSON blob
    print_bytes(&json_result);

    let json_val: JsonValue = simple_json::parse_json(
      &core::str::from_utf8(&json_result).unwrap())
      .map_err(|_| "JSON parsing error")?;

    Ok(json_val)
  }

  fn fetch_price<'a>(
    block: T::BlockNumber,
    sym: &'a [u8],
    remote_src: &'a [u8],
    remote_url: &'a [u8]
  ) -> dispatch::DispatchResult {
    debug::info!("fetch price: {:?}:{:?}",
      core::str::from_utf8(sym).unwrap(),
      core::str::from_utf8(remote_src).unwrap()
    );

    let json = Self::fetch_json(remote_url)?;

    let price = match remote_src {
      src if src == b"coincap" => Self::fetch_price_from_coincap(json)
        .map_err(|_| "fetch_price_from_coincap error"),
      src if src == b"cryptocompare" => Self::fetch_price_from_cryptocompare(json)
        .map_err(|_| "fetch_price_from_cryptocompare error"),
      _ => Err("Unknown remote source"),
    }?;

    let call = Call::record_price(
      block,
      (sym.to_vec(), remote_src.to_vec(), remote_url.to_vec()),
      price
    );

    // Unsigned tx
    T::SubmitUnsignedTransaction::submit_unsigned(call)
      .map_err(|_| DispatchError::Other("fetch_price: submit_signed(call) error"))

    // Signed tx
    // let local_accts = T::SubmitTransaction::find_local_keys(None);
    // let (local_acct, local_key) = local_accts[0];
    // debug::info!("acct: {:?}", local_acct);
    // T::SignAndSubmitTransaction::sign_and_submit(call, local_key);

    // T::SubmitSignedTransaction::submit_signed(call);
    // Ok(())
  }

  fn vecchars_to_vecbytes<I: IntoIterator<Item = char> + Clone>(it: &I) -> Vec<u8> {
    it.clone().into_iter().map(|c| c as u8).collect::<_>()
  }

  fn fetch_price_from_cryptocompare(json_val: JsonValue) -> StdResult<u64> {
    // Expected JSON shape:
    //   r#"{"USD": 7064.16}"#;

    let val_f64: f64 = json_val.get_object()[0].1.get_number_f64();
    let val_u64: u64 = (val_f64 * 10000.).round() as u64;
    Ok(val_u64)
  }

  fn fetch_price_from_coincap(json_val: JsonValue) -> StdResult<u64> {
    // Expected JSON shape:
    //   r#"{"data":{"priceUsd":"8172.2628346190447316"}}"#;

    const PRICE_KEY: &[u8] = b"priceUsd";
    let data = json_val.get_object()[0].1.get_object();

    let (_, v) = data.iter()
      .filter(|(k, _)| PRICE_KEY.to_vec() == Self::vecchars_to_vecbytes(k))
      .nth(0)
      .ok_or("fetch_price_from_coincap: JSON does not conform to expectation")?;

    // `val` contains the price, such as "222.333" in bytes form
    let val_u8: Vec<u8> = v.get_bytes();

    // Convert to number
    let val_f64: f64 = core::str::from_utf8(&val_u8)
      .map_err(|_| "fetch_price_from_coincap: val_f64 convert to string error")?
      .parse::<f64>()
      .map_err(|_| "fetch_price_from_coincap: val_u8 parsing to f64 error")?;
    let val_u64 = (val_f64 * 10000.).round() as u64;
    Ok(val_u64)
  }

  fn aggregate_pp<'a>(block: T::BlockNumber, sym: &'a [u8], freq: usize) -> dispatch::DispatchResult {
    let ts_pp_vec = <TokenSrcPPMap>::get(sym);

    // use the last `freq` number of prices and average them
    let amt: usize = if ts_pp_vec.len() > freq { freq } else { ts_pp_vec.len() };
    let pp_inds: &[u32] = ts_pp_vec.get((ts_pp_vec.len() - amt)..ts_pp_vec.len())
      .ok_or("aggregate_pp: extracting TokenSrcPPMap error")?;

    let src_pp_vec: Vec<_> = Self::src_price_pts();
    let price_sum: u64 = pp_inds.iter().fold(0, |mem, ind| mem + src_pp_vec[*ind as usize].1);
    let price_avg: u64 = (price_sum as f64 / amt as f64).round() as u64;

    // submit onchain call for aggregating the price
    let call = Call::record_agg_pp(block, sym.to_vec(), price_avg);

    // Unsigned tx
    T::SubmitUnsignedTransaction::submit_unsigned(call)
      .map_err(|_| DispatchError::Other("aggregate_pp: submit_signed(call) error"))

    // Signed tx
    // T::SubmitSignedTransaction::submit_signed(call);
    // Ok(())
  }
}

impl<T: Trait> support::unsigned::ValidateUnsigned for Module<T> {
  type Call = Call<T>;

  fn validate_unsigned(call: &Self::Call) -> TransactionValidity {

    match call {
      Call::record_price(block, (sym, remote_src, ..), price) => Ok(ValidTransaction {
        priority: 0,
        requires: vec![],
        provides: vec![(block, sym, remote_src, price).encode()],
        longevity: TransactionLongevity::max_value(),
        propagate: true,
      }),
      Call::record_agg_pp(block, sym, price) => Ok(ValidTransaction {
        priority: 0,
        requires: vec![],
        provides: vec![(block, sym, price).encode()],
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
