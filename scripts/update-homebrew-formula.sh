#!/bin/sh
set -eu

if [ "$#" -ne 5 ]; then
    echo "usage: $0 VERSION OWNER/REPO ARM64_SHA256 X86_64_SHA256 OUTPUT" >&2
    exit 2
fi

version=$1
repository=$2
arm_sha=$3
x86_sha=$4
output=$5

sed \
    -e "s|@VERSION@|${version}|g" \
    -e "s|@REPOSITORY@|${repository}|g" \
    -e "s|@ARM64_SHA256@|${arm_sha}|g" \
    -e "s|@X86_64_SHA256@|${x86_sha}|g" \
    packaging/homebrew/kalendar.rb.template > "$output"

