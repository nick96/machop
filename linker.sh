#!/usr/bin/env bash

# Make sure the linker is build so we're always using the latest version
here="$(dirname $(realpath $0))"
pushd $here
cargo build --quiet
if [[ $? != 0 ]]
then
    exit 1
fi
popd &>/dev/null

# Enable tracing so that that we can copy the linker invocation if
# need be.
set -x

bin="$here/target/debug/machop"
if [[ "$DEBUG" == "1" ]]
then
    rust-lldb $bin -- $@
else
    # Run the linker binary, passing through all the given args.
    RUST_LOG=${RUST_LOG:-warn,machop=debug} $bin $@
fi


