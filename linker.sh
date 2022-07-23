#!/usr/bin/env bash

# Make sure the linker is build so we're always using the latest version
here="$(dirname $(realpath $0))"
pushd $here
cargo build --quiet
popd &>/dev/null

# Enable tracing so that that we can copy the linker invocation if
# need be.
set -x

# Run the linker binary, passing through all the given args.
RUST_LOG=warn,nicks_linker=debug $here/target/debug/nicks-linker $@
