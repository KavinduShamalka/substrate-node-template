#![cfg(test)]

use crate::{Module, Trait};
use primitives::{H256};
use support::{impl_outer_origin, impl_outer_dispatch, parameter_types, weights::Weight};
use sp_runtime::{
  traits::{BlakeTwo256, IdentityLookup},
  testing::{Header, TestXt},
  Perbill
};

impl_outer_origin! {
  pub enum Origin for TestRuntime {}
}

impl_outer_dispatch! {
  pub enum Call for TestRuntime where origin: Origin {
    price_fetch::PriceFetchModule,
  }
}

// For testing the module, we construct most of a mock runtime. This means
// first constructing a configuration type (`Test`) which `impl`s each of the
// configuration traits of modules we want to use.
#[derive(Clone, Eq, PartialEq)]
pub struct TestRuntime;

parameter_types! {
  pub const BlockHashCount: u64 = 250;
  pub const MaximumBlockWeight: Weight = 1024;
  pub const MaximumBlockLength: u32 = 2 * 1024;
  pub const AvailableBlockRatio: Perbill = Perbill::from_percent(75);
}

impl system::Trait for TestRuntime {
  type Origin = Origin;
  type Call = Call;
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
  type ModuleToIndex = ();
}

impl timestamp::Trait for TestRuntime {
  type Moment = u64;
  type OnTimestampSet = ();
  type MinimumPeriod = ();
}

pub type Extrinsic = TestXt<Call, ()>;
type SubmitPFTransaction = system::offchain::TransactionSubmitter<(), Call, Extrinsic>;

parameter_types! {
  pub const BlockFetchDur: u64 = 1;
}

pub type PriceFetchModule = Module<TestRuntime>;

impl Trait for TestRuntime {
  type Event = ();
  type Call = Call;
  type SubmitUnsignedTransaction = SubmitPFTransaction;

  // Wait period between automated fetches. Set to 0 disable this feature.
  //   Then you need to manucally kickoff pricefetch
  type BlockFetchDur = BlockFetchDur;
}

// This function basically just builds a genesis storage key/value store according to
// our desired mockup.
pub fn new_test_ext() -> runtime_io::TestExternalities {
  system::GenesisConfig::default().build_storage::<TestRuntime>().unwrap().into()
}
