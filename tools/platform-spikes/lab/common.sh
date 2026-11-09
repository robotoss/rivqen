# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Shared helpers of the lab scripts. Source this file after lab-env.sh, with the script arguments:
#   . "$HERE/common.sh"
# Dry run: pass --dry-run as the first argument or set LAB_DRY_RUN=1. A dry run prints the commands
# that change something and runs none of them.

LAB_DRY_RUN="${LAB_DRY_RUN:-0}"
if [ "${1:-}" = "--dry-run" ]; then
  LAB_DRY_RUN=1
  shift
fi
export LAB_DRY_RUN

# Run a command that changes state, or print it in dry-run mode.
run() {
  if [ "$LAB_DRY_RUN" = 1 ]; then
    echo "dry-run: $*"
    return 0
  fi
  "$@"
}

# The lab root must be a directory below a volume (DL-138).
require_lab_root() {
  case "$LAB_HOME" in
    /Volumes/*/*) ;;
    *)
      echo "$0: LAB_HOME must be a directory on an external volume (/Volumes/<name>/...), got $LAB_HOME" >&2
      exit 2
      ;;
  esac
  [ -d "$(dirname "$LAB_HOME")" ] || {
    echo "$0: the volume of $LAB_HOME is not mounted" >&2
    exit 2
  }
}

# Free space of the system volume in whole GB.
system_free_gb() {
  df -k / | awk 'NR==2 { printf "%d", $4 / 1048576 }'
}
