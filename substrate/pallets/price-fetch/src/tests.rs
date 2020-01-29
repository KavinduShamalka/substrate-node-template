#![cfg(test)]

/// tests for this module

// Test cases:
//  1. record_price if called store item in storage
//  2. record_price can only be called from unsigned tx
//  3. with multiple record_price of same sym inserted. On next cycle, the average of the price is calculated
//  4. can fetch for BTC, parse the JSON blob and get a price > 0 out

use crate::mock::*;

#[test]
fn it_works_for_default_value() {
  new_test_ext().execute_with(|| {
    assert_eq!(1, 1);
  });
}
