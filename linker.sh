#!/usr/bin/env bash

# Make sure the linker is build so we're always using the latest version
here="$(dirname $(realpath $0))"
pushd $here
cargo build --quiet
popd

# Run the linker binary, passing through all the given args.
$here/target/debug/nicks-linker $@
