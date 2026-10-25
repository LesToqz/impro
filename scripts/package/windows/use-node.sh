#!/usr/bin/env bash
# Source this from an msys2 step to put the runner image's Node.js on PATH (the MSYS2 shell PATH is minimal and
# does not contain it) and log `node --version`. Fails with a clear message when Node.js cannot be found.
node_exe=""
# Windows PATH lookup first (the MSYS2 login shell does not inherit it).
if found=$(cmd.exe //c "where node.exe" 2>/dev/null | tr -d '\r' | head -n 1) && [[ -n $found ]]; then
  node_exe=$(cygpath -u "$found")
fi
if [[ -z $node_exe || ! -x $node_exe ]]; then
  node_exe=""
  for candidate in "${PROGRAMFILES:-}/nodejs/node.exe" "${RUNNER_TOOL_CACHE:-}"/node/*/x64/node.exe; do
    if [[ -n $candidate && -x $candidate ]]; then
      node_exe=$candidate
      break
    fi
  done
fi
if [[ -z $node_exe ]]; then
  echo "::error::node.exe not found (tried 'where node.exe', Program Files/nodejs and the runner tool cache)"
  return 1
fi
node_dir=$(dirname "$node_exe")
export PATH="$node_dir:$PATH"
echo "node: $node_exe"
node --version
