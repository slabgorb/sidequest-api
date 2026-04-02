#!/bin/sh
# Test helper: dumps all environment variables, ignoring all arguments.
# Used by otel_injection_story_21_4_tests.rs to verify env var inheritance.
env
