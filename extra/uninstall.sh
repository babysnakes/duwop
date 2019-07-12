#!/bin/bash -e

# This script will remove the duwopctl executable link.

name="duwopctl"
RED='\033[0;31m'
BOLD='\033[1m'
NC='\033[0m' # No Color

function finale() {
  echo
  echo "To completely remove 'duwop' files from your system (including configurations, logs etc) run:"
  echo "rm -rf $HOME/.duwop"
  echo
}

echo "This script will remove 'duwop' from your system"

duwopctl=$(type -p duwopctl) || (
  echo -e "${RED}Sorry,${NC} couldn't find 'duwopctl executable in your PATH. you can search and remove it manually."
  finale
  exit 1
)

if [[ ! -L $duwopctl ]]; then
  echo -e "${RED}Sorry,${NC} $duwopctl should be a link but it doesn't! Please delete it manually"
  finale
  exit 1
fi

directory=$(dirname $duwopctl)
if [[ -w $directory ]]; then
  unlink $duwopctl
  finale
else
  echo -e "${BOLD}Note:${NC} you might be prompted for your (sudo) password"
  sudo unlink $duwopctl
  finale
fi
