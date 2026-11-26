#!/bin/bash
# ACTIONS_RUNNER_HOOK_JOB_STARTED (the runner passes no arguments to a hook).
exec /bin/bash "$(dirname "$0")/job-cleanup.sh" started
