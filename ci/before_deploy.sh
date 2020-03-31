#!/bin/bash

# package the build artifacts
# Based on https://github.com/BurntSushi/ripgrep/blob/master/.travis.yml

# See: https://vaneyckt.io/posts/safer_bash_scripts_with_set_euxo_pipefail/
set -eux -o pipefail

# Generate artifacts for release
mk_artifacts() {
        cargo build --target "$TARGET" --release
}

mk_tarball() {
        # Create a temporary dir that contains our staging area.
        # $tmpdir/$name is what eventually ends up as the deployed archive.
        local tmpdir="$(mktemp -d)"
        local name="${PROJECT_NAME}-${TRAVIS_TAG}-${TARGET}"
        local staging="$tmpdir/$name"
        mkdir -p "$staging"/{bin,doc}
        # The deployment directory is where the final archive will reside.
        # This path is known by the .travis.yml configuration.
        local out_dir="$HOME/deployment"
        mkdir -p "$out_dir"
        # Find the correct (most recent) Cargo "out" directory. The out directory
        # contains shell completion files and the man page.
        local cargo_out_dir="$(cargo_out_dir "target/$TARGET")"

        # TODO: Strip binaries?

        # Copy the binaries:
        cp "target/$TARGET/release/stmgr" "$staging/bin/stmgr"
        cp "target/$TARGET/release/strelay" "$staging/bin/strelay"
        cp "target/$TARGET/release/stindex" "$staging/bin/stindex"
        cp "target/$TARGET/release/stnode" "$staging/bin/stnode"
        cp "target/$TARGET/release/stctrl" "$staging/bin/stctrl"
        cp "target/$TARGET/release/stcompact" "$staging/bin/stcompact"

        # Copy README file:
        cp README.md "$staging/"

        # Copy all licenses:
        cp LICENSE-* "$staging/"

        # Copy mkdocs documentation:
        cp -R "doc/." "$staging/doc/"

        # TODO: Copy man pages?
        # TODO: Copy shell completion files.

        (cd "$tmpdir" && tar czf "$out_dir/$name.tar.gz" "$name")
        rm -rf "$tmpdir"
}

main() {
    mk_artifacts
    mk_tarball
}

main
