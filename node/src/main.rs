//! Substrate Node Template CLI library.

#![warn(missing_docs)]
#![warn(unused_extern_crates)]

mod chain_spec;
#[macro_use]
mod service;
mod cli;

pub use sc_cli::{VersionInfo, IntoExit, error};

fn main() -> Result<(), cli::error::Error> {
	let version = VersionInfo {
		name: "Offchain Worker - Price Fetch",
		commit: env!("VERGEN_SHA_SHORT"),
		version: env!("CARGO_PKG_VERSION"),
		executable_name: "offchain-pricefetch",
		author: "Jimmy Chu",
		description: "Offchain Worker - Price Fetch",
		support_url: "https://github.com/jimmychu0807/substrate-offchain-pricefetch",
	};

	cli::run(std::env::args(), cli::Exit, version)
}
