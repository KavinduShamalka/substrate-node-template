/// A runtime module template with necessary imports

/// Feel free to remove or edit this file as needed.
/// If you change the name of this file, make sure to update its references in runtime/src/lib.rs
/// If you remove this file, you can remove those references


/// For more guidance on Substrate modules, see the example module
/// https://github.com/paritytech/substrate/blob/master/srml/example/src/lib.rs

// We have to import a few things
use rstd::{prelude::*, convert::TryInto};
use primitives::crypto::KeyTypeId;
use support::{decl_module, decl_storage, decl_event, dispatch, debug};
use system::{ensure_signed, offchain, offchain::SubmitUnsignedTransaction};
use simple_json::{ self, json::JsonValue };

use runtime_io::{ self, misc::print_utf8 as print_bytes };
use codec::{Encode, Decode};
use sp_runtime::{
  offchain::http,
  transaction_validity::{
    TransactionValidity, TransactionLongevity, ValidTransaction, InvalidTransaction
  }
};

type StdResult<T> = core::result::Result<T, &'static str>;

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
pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"ofpf");

pub mod crypto {
  pub use super::KEY_TYPE;
  use sp_runtime::app_crypto::{app_crypto, sr25519};
  app_crypto!(sr25519, KEY_TYPE);
}

// This automates price fetching every certain blocks. Set to 0 disable this feature.
//   Then you need to manucally kickoff pricefetch
pub const BLOCK_FETCH_DUR: u64 = 3;

pub const FETCHED_CRYPTOS: [(&'static [u8], &'static [u8], &'static [u8]); 1] = [
  (b"BTC", b"coincap",
    b"https://api.coincap.io/v2/assets/bitcoin"),
  // (b"BTC", b"coinmarketcap",
  //  b"https://sandbox-api.coinmarketcap.com/v1/cryptocurrency/quotes/latest?CMC_PRO_API_KEY=2e6d8847-bcea-4999-87b1-ad452efe4e40&symbol=BTC"),
  // (b"BTC", b"cryptocompare",
  //   b"https://min-api.cryptocompare.com/data/price?fsym=BTC&tsyms=USD"),
  // (b"ETH", b"coincap",
  //  b"https://api.coincap.io/v2/assets/ethereum"),
  // (b"ETH", b"coinmarketcap",
  //  b"https://sandbox-api.coinmarketcap.com/v1/cryptocurrency/quotes/latest?CMC_PRO_API_KEY=2e6d8847-bcea-4999-87b1-ad452efe4e40&symbol=ETH"),
];

/// The module's configuration trait.
pub trait Trait: timestamp::Trait + system::Trait {
  /// The overarching event type.
  type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
  type Call: From<Call<Self>>;

  type SubmitTransaction: offchain::SubmitSignedTransaction<Self, <Self as Trait>::Call>;
  type SubmitUnsignedTransaction: offchain::SubmitUnsignedTransaction<Self, <Self as Trait>::Call>;
}

decl_event!(
  pub enum Event<T> where
    Moment = <T as timestamp::Trait>::Moment {

    PriceFetched(Vec<u8>, Vec<u8>, Moment, Price),
    AggregatedPrice(Vec<u8>, Moment, Price),
  }
);

// This module's storage items.
decl_storage! {
  trait Store for Module<T: Trait> as PriceFetch {
    UpdateAggPP get(update_agg_pp): linked_map Vec<u8> => bool = false;

    // storage about source price points
    // mapping of ID(index) -> (timestamp, `Price` obj)
    SrcPricePoints get(src_price_pts): Vec<(T::Moment, Price)>;

    // mapping of token sym -> pp_id
    TokenSrcPPMap: map Vec<u8> => Vec<u32>;

    // mapping of remote_src -> pp_id
    RemoteSrcPPMap: map Vec<u8> => Vec<u32>;

    // storage about aggregated price points (calculated with our logic)
    AggPricePoints get(agg_price_pts): Vec<(T::Moment, Price)>;
    TokenAggPPMap: map Vec<u8> => Vec<u32>;
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
      origin,
      crypto_info: (Vec<u8>, Vec<u8>, Vec<u8>),
      price: Price
    ) -> dispatch::Result {
      let (sym, remote_src) = (crypto_info.0, crypto_info.1);
      let now = <timestamp::Module<T>>::get();

      // Debug printout
      debug::info!("-- record_price: {:?}, {:?}, {:?}",
        core::str::from_utf8(&sym).unwrap(),
        core::str::from_utf8(&remote_src).unwrap(),
        price
      );

      // Spit out an event and Add to storage
      Self::deposit_event(RawEvent::PriceFetched(
        sym.clone(), remote_src.clone(), now.clone(), price.clone()));

      let price_pt = (now, price);
      // The index serves as the ID
      let pp_id: u32 = Self::src_price_pts().len().try_into().unwrap();
      <SrcPricePoints<T>>::mutate(|vec| vec.push(price_pt));
      <TokenSrcPPMap>::mutate(sym.clone(), |token_vec| token_vec.push(pp_id));
      <RemoteSrcPPMap>::mutate(remote_src, |rs_vec| rs_vec.push(pp_id));

      // set the flag to kick off update aggregated pricing in offchain call
      <UpdateAggPP>::mutate(sym.clone(), |flag| *flag = true);

      Ok(())
    }

    pub fn record_agg_pp(origin, sym: Vec<u8>, price: Price) -> dispatch::Result {
      // Debug printout
      debug::info!("-- record_agg_pp: {}: {:?}",
        core::str::from_utf8(&sym).unwrap(),
        price);

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
      <UpdateAggPP>::mutate(sym.clone(), |flag| *flag = false);

      Ok(())
    }

    fn offchain_worker(block: T::BlockNumber) {
      let current_block = TryInto::<u64>::try_into(block).ok().unwrap();

      // Type I task: fetch_price
      if BLOCK_FETCH_DUR > 0 && current_block % BLOCK_FETCH_DUR == 0 {
        for (sym, remote_src, remote_url) in FETCHED_CRYPTOS.iter() {
          if let Err(e) = Self::fetch_price(*sym, *remote_src, *remote_url) {
            debug::error!("Error fetching: {:?}, {:?}: {}", sym, remote_src, e);
          }
        }
      }

      // Type II task: aggregate price
      <UpdateAggPP>::enumerate()
        // filter those to be updated
        .filter(|(_, update)| *update)
        .for_each(|(sym, _)| {
          if let Err(e) = Self::aggregate_pp(&sym) {
            debug::error!("Error aggregating price of {:?}: {}",
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

  fn fetch_price<'a>(sym: &'a [u8], remote_src: &'a [u8], remote_url: &'a [u8]) -> dispatch::Result {
    debug::info!("fetch price: {:?}:{:?}",
      core::str::from_utf8(sym).unwrap(),
      core::str::from_utf8(remote_src).unwrap()
    );

    let json = Self::fetch_json(remote_url)?;

    let price = match remote_src {
      src if src == b"coincap" => Self::fetch_price_from_coincap(json)
        .map_err(|_| "fetch_price_from_coincap error"),
      src if src == b"coinmarketcap" => Self::fetch_price_from_coinmarketcap(json)
        .map_err(|_| "fetch_price_from_coinmarketcap error"),
      _ => return Err("Unknown remote source"),
    }?;

    let call = Call::record_price((sym.to_vec(), remote_src.to_vec(), remote_url.to_vec()), price);
    T::SubmitUnsignedTransaction::submit_unsigned(call)
      .map_err(|_| "fetch_price: submit_unsigned_call error")
  }

  fn vecchars_to_vecbytes<I: IntoIterator<Item = char> + Clone>(it: &I) -> Vec<u8> {
    it.clone().into_iter().map(|c| c as u8).collect::<_>()
  }

  fn round_highest_digits(digits: &[u8], len: usize) -> StdResult<u32> {
    const ROUND_OFF_BOUNDARY: u32 = 4;
    // pushing 0 to the end
    if digits.len() <= len {
      let mut digits_vec = digits.to_vec();
      for n in 0..(len - digits.len()) {
        digits_vec.push(b'0');
      }
      return core::str::from_utf8(&digits_vec).unwrap().parse::<u32>()
        .map_err(|_| "round_highest_digits: parsing to u32 error");
    }
    // `digits.len()` > `len`
    let wanted_digits = digits.get(0..len).unwrap();
    let last_digit = (digits.get(len..len+1).unwrap()[0] as char).to_digit(10).unwrap();
    let mut digits_u32 = core::str::from_utf8(wanted_digits).unwrap().parse::<u32>()
      .map_err(|_| "round_highest_digits: parsing to u32 error")?;
    if last_digit > ROUND_OFF_BOUNDARY {
      digits_u32 += 1;
    }
    return Ok(digits_u32);
  }

  fn fetch_price_from_coincap(json_val: JsonValue) -> StdResult<Price> {
    // This is the expected JSON path:
    // r#"{"data":{"priceUsd":"8172.2628346190447316"}}"#;

    let data = json_val.get_object()[0].1.get_object();
    let price_bytes = b"priceUsd";

    let (_, v) = data.iter()
      .filter(|(k, _)| {
        let k_bytes = Self::vecchars_to_vecbytes(k);
        &k_bytes == price_bytes
      })
      .nth(0).expect("Should have `priceUsd` field");

    // `val` contains the price, such as "222.333" in bytes form
    let val: Vec<u8> = v.get_bytes();
    let dot_pos = val.iter().position(|&i| i == ('.' as u8)).unwrap();
    let dollars_byte: Vec<u8> = val.get(0..dot_pos).unwrap().to_vec();
    let cents_byte: Vec<u8> = val.get((dot_pos + 1)..).unwrap().to_vec();

    // Convert to number
    let dollars_u32: u32 = core::str::from_utf8(&dollars_byte).unwrap()
      .parse::<u32>().unwrap();
    let cents_u32: u32 = Self::round_highest_digits(&cents_byte, 4).unwrap();
    Ok(Price::new(dollars_u32, cents_u32, None))
  }

  fn fetch_price_from_coinmarketcap(_json: JsonValue) -> StdResult<Price> {
    print_bytes(b"-- fetch_price_from_coinmarketcap");
    Ok(Price::new(100, 3500, None))
  }

  fn aggregate_pp<'a>(sym: &'a [u8]) -> dispatch::Result {
    // TODO: enhance the aggregation logic
    //   Currently, fetch the last price point of the token and use as its aggregated price
    let ts_pp_vec = <TokenSrcPPMap>::get(sym);
    let pp_id: usize = *ts_pp_vec.last().unwrap() as usize;

    let src_pp_vec = Self::src_price_pts();
    let price = src_pp_vec[pp_id].1.clone();

    debug::info!("-- aggregate_pp: {}, {:?}",
      core::str::from_utf8(sym).unwrap(), price);

    // submit onchain call for aggregating the price
    let call = Call::record_agg_pp(sym.to_vec(), price);
    T::SubmitUnsignedTransaction::submit_unsigned(call)
      .map_err(|_| "aggregate_pp: submit_unsigned_call error")
  }
}

#[allow(deprecated)]
impl<T: Trait> support::unsigned::ValidateUnsigned for Module<T> {
  type Call = Call<T>;

  fn validate_unsigned(call: &Self::Call) -> TransactionValidity {
    let now = <timestamp::Module<T>>::get();

    debug::info!("validate_unsigned");
    debug::info!("{:?}", TryInto::<u64>::try_into(now).ok().unwrap());

    match call {
      Call::record_price((sym, remote_src, ..), price) => Ok(ValidTransaction {
        priority: 0,
        requires: vec![],
        provides: vec![(now, sym, remote_src, price).encode()],
        longevity: TransactionLongevity::max_value(),
        propagate: true,
      }),
      Call::record_agg_pp(sym, price) => Ok(ValidTransaction {
        priority: 0,
        requires: vec![],
        provides: vec![(now, sym, price).encode()],
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
