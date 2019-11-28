// Ensure we're `no_std` when compiling for Wasm. Otherwise our `Vec` and operations
// on it will fail with `invalid`.
#![cfg_attr(not(feature = "std"), no_std)]

// We have to import a few things
use rstd::prelude::*;
use app_crypto::RuntimeAppPublic;
use support::{decl_module, decl_event, decl_storage, print, StorageValue, dispatch::Result};
use system::{ensure_signed, ensure_root, ensure_none};
use system::offchain::{SubmitSignedTransaction, SubmitUnsignedTransaction};
use codec::{Encode, Decode};
use sr_primitives::{
  transaction_validity::{
    TransactionValidity, TransactionLongevity, ValidTransaction, InvalidTransaction
  }
};

pub const KEY_TYPE: app_crypto::KeyTypeId = app_crypto::KeyTypeId(*b"ofcb");

/// The module's main configuration trait.
pub trait Trait: timestamp::Trait + system::Trait  {
  /// The regular events type, we use to emit the `Ack`
  type Event:From<Event<Self>> + Into<<Self as system::Trait>::Event>;

  /// A dispatchable call type. We need to define it for the offchain worker to
  /// reference the `pong` function it wants to call.
  type Call: From<Call<Self>>;

  /// Let's define the helper we use to create signed transactions with
  type SubmitTransaction: SubmitSignedTransaction<Self, <Self as Trait>::Call>;

  type SubmitUnsignedTransaction: SubmitUnsignedTransaction<Self, <Self as Trait>::Call>;

  /// The local keytype
  type KeyType: RuntimeAppPublic + From<Self::AccountId> + Into<Self::AccountId> + Clone;
}

#[cfg_attr(feature = "std", derive(PartialEq, Eq, Debug))]
#[derive(Encode, Decode)]
pub enum OffchainRequest<T: system::Trait> {
  /// If an authorised offchain worker sees this ping, it shall respond with a `pong` call
  Ping(u8,  <T as system::Trait>::AccountId)
}

// We use the regular Event type to sent the final ack for the nonce
decl_event!(
  pub enum Event<T> where AccountId = <T as system::Trait>::AccountId {
    /// When we received a Pong, we also Ack it.
    Ack(u8, AccountId),
    AckNoAuthor(u8),
  }
);

decl_storage! {
  trait Store for Module<T: Trait> as OffchainCb {
    /// Requests made within this block execution
    OcRequests get(oc_requests): Vec<OffchainRequest<T>>;
    /// The current set of keys that may submit pongs
    Authorities get(authorities) config(): Vec<T::AccountId>;
  }
}

decl_module! {
  pub struct Module<T: Trait> for enum Call where origin: T::Origin {
    /// Initializing events
    fn deposit_event() = default;

    /// Clean the state on initialisation of a block
    fn on_initialize(_now: T::BlockNumber) {
      // At the beginning of each block execution, system triggers all
      // `on_initialize` functions, which allows us to set up some temporary state or - like
      // in this case - clean up other states
      <Self as Store>::OcRequests::kill();
    }


    /// The entry point function: storing a `Ping` offchain request with the given `nonce`.
    pub fn ping(origin, nonce: u8) -> Result {
      // It first ensures the function was signed, then it store the `Ping` request
      // with our nonce and author. Finally it results with `Ok`.
      let who = ensure_signed(origin)?;
      print("pinging");
      <Self as Store>::OcRequests::mutate(|v| v.push(OffchainRequest::Ping(nonce, who)));
      Ok(())
    }

    /// Called from the offchain worker to respond to a ping
    pub fn pong_signed(origin, nonce: u8) -> Result {
      // We don't allow anyone to `pong` but only those authorised in the `authorities`
      // set at this point. Therefore after ensuring this is signed, we check whether
      // that given author is allowed to `pong` is. If so, we emit the `Ack` event,
      // otherwise we've just consumed their fee.
      let author = ensure_signed(origin)?;

      runtime_io::print_utf8(b"pong_signed: called");

      if Self::is_authority(&author) {
        runtime_io::print_utf8(b"pong_signed: is_authority");
        Self::deposit_event(RawEvent::Ack(nonce, author));
      }

      Ok(())
    }

    pub fn pong_unsigned(origin, nonce: u8) -> Result {
      ensure_none(origin)?;
      runtime_io::print_utf8(b"pong_unsigned: called");
      Self::deposit_event(RawEvent::AckNoAuthor(nonce));
      Ok(())
    }

    // Runs after every block within the context and current state of said block.
    fn offchain_worker(_now: T::BlockNumber) {
      // As `pongs` are only accepted by authorities, we only run this code,
      // if a valid local key is found, we could submit them with.
      for e in <Self as Store>::OcRequests::get() {
        match e {
          OffchainRequest::Ping(nonce, who) => {
            Self::offchain_unsigned(nonce).expect("offchain_unsigned request should succeed");
            Self::offchain_signed(&who, nonce).expect("offchain_signed request should succeed");

            if let Some(key) = Self::authority_id() {
              Self::offchain_signed(&key, nonce).expect("offchain_signed 2 request should succeed");
            }
          }
        }
      }
    }

    // Simple authority management: add a new authority to the set of keys that
    // are allowed to respond with `pong`.
    pub fn add_authority(origin, who: T::AccountId) -> Result {
      // In practice this should be a bit cleverer, but for this example it is enough
      // that this is protected by a root-call (e.g. through governance like `sudo`).
      let _me = ensure_root(origin)?;

      if !Self::is_authority(&who) {
        <Authorities<T>>::mutate(|l| l.push(who));
      }

      Ok(())
    }
  }
}

// We've moved the  helper functions outside of the main declaration for brevity.
impl<T: Trait> Module<T> {
  /// Responding to as the given account to a given nonce by calling `pong` as a
  /// newly signed and submitted trasnaction
  fn offchain_signed(key: &T::AccountId, nonce: u8) -> Result {
    print("offchain_signed");
    let call = Call::pong_signed(nonce);
    T::SubmitTransaction::sign_and_submit(call, key.clone().into())
      .map_err(|_| "offchain_signed error")
  }

  fn offchain_unsigned(nonce: u8) -> Result {
    print("offchain_unsigned");
    let call = Call::pong_unsigned(nonce);
    T::SubmitUnsignedTransaction::submit_unsigned(call)
      .map_err(|_| "offchain_unsigned error")
  }

  /// Helper that confirms whether the given `AccountId` can sign `pong` transactions
  fn is_authority(who: &T::AccountId) -> bool {
    Self::authorities().into_iter().find(|i| i == who).is_some()
  }

  /// Find a local `AccountId` we can sign with, that is allowed to `pong`
  fn authority_id() -> Option<T::AccountId> {
    // Find all local keys accessible to this app through the localised KeyType.
    // Then go through all keys currently stored on chain and check them against
    // the list of local keys until a match is found, otherwise return `None`.
    let local_keys = T::KeyType::all().iter().map(
        |i| (*i).clone().into()
      ).collect::<Vec<T::AccountId>>();

    // ASK: how can I print what's in `local_keys`?
    // print(local_keys);

    Self::authorities().into_iter().find_map(|authority| {
      if local_keys.contains(&authority) {
        print("found authority");
        Some(authority)
      } else {
        print("authority not found");
        None
      }
    })
  }
}

impl<T: Trait> support::unsigned::ValidateUnsigned for Module<T> {
  type Call = Call<T>;

  fn validate_unsigned(call: &Self::Call) -> TransactionValidity {
    if let Call::pong_unsigned(nonce) = call {
      print("validate_unsigned: true");

      let now = <timestamp::Module<T>>::get();

      return Ok(ValidTransaction {
        priority: 0,
        requires: vec![],
        provides: vec![(nonce, now).encode()],
        longevity: TransactionLongevity::max_value(),
        propagate: true,
      });
    }

    print("validate_unsigned: false");
    InvalidTransaction::Call.into()
  }
}
