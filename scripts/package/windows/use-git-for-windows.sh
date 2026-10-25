#!/usr/bin/env bash
# Source this from an msys2 step to put Git for Windows ahead of the MSYS2 git on PATH, then log which git
# wins and its configuration. Users run impro with Git for Windows, so the tests should too.
git_cmd=$(cygpath -u "$PROGRAMFILES/Git/cmd")
if [[ ! -x $git_cmd/git.exe ]]; then
  echo "::error::Git for Windows not found at $git_cmd"
  return 1
fi
export PATH="$git_cmd:$PATH"
command -v git
git --version
git config --show-scope --show-origin --list | head -20 || true
