#!/bin/bash -e

# Usage: ./release.sh [ optional-tag ]
# If optional-tag not supplied, it will try to get HEAD's tag or fail!

function clean() {
  rm -rf target/duwop target/duwop-*.tar.gz
}

function prepare() {
  cargo build --release
  clean
  mkdir -p target/duwop
  cp target/release/{duwop,duwopctl} target/duwop/
  cp extra/{install.sh,uninstall.sh} target/duwop/
  chmod +x target/duwop/*
}

function archive() {
  match=$1
  if [[ -z "$match" ]]; then
    match=$(git describe --exact-match)
  fi
  tar czf target/duwop-bin-${match}.tar.gz -C target/ duwop
}

case "$1" in
prepare)
  prepare
  ;;
archive)
  prepare
  archive "$2"
  ;;
clean)
  clean
  ;;

*)
  echo "Usage: $0 [ prepare | archive | clean ]"
  echo "where:"
  echo "- prepare: create directory with artifacts"
  echo "- archive: archive the created directory"
  echo "- clean release artifacts"
  ;;
esac
