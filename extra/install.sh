#!/bin/bash -e

# Installs duwop binaries to '$HOME/.duwop/bin' Assumes that this script is at
# the same folder as `duwop` and `duwopctl`.

BOLD='\033[1m'
GREEN='\033[32m'
NC='\033[0m' # No Color

source_dir=`dirname $0`
target_dir="$HOME/.duwop/bin"
default_path_dir="/usr/local/bin"
prefix="sudo "

mkdir -p $target_dir
for bin in duwop duwopctl uninstall.sh; do
  install ${source_dir}/$bin ${target_dir}/
done

echo "Please specify a directory in your path to link duwopctl to"
read -p "[$default_path_dir]: " directory
[[ -z "$directory" ]] && directory=${default_path_dir}

if [[ -w $directory ]]; then
  ln -s ${target_dir}/duwopctl $directory/
else
  echo -e "${BOLD}Note:${NC} you might be prompted for your (sudo) password"
  sudo ln -s ${target_dir}/duwopctl $directory/
fi

echo -e "${GREEN}->${NC} Installation is complete :)"
echo -e "${GREEN}->${NC} Please run '${directory}/duwopctl help setup' for setup instructions"
