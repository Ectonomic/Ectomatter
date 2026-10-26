#!/bin/bash
# Every boundary Ectomatter claims, tried, the known failures too: which hold
# and which don't yet. A boundary that doesn't hold is marked in its test as a
# known failure and listed in docs/deferred.md. Builds and runs the tests in
# release, as the app's other tests run.
set -uo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
# the checks of the source (boundaries.rs); the ones that run a made-up core
# or plugin against the app, each named for the boundary it tries (beside the
# code it tries: core_host.rs, hosted.rs, keyboard.rs, launcher.rs,
# updates.rs, lcd_page.rs, ecto_settings.rs); then Ectomatter run as a
# stranger's computer has it (tests/stranger.rs)
runs=(
  a_package_past_its_time_is_stopped_and_started_again
  a_package_out_of_memory_or_crashed_is_stopped_and_started_again
  a_timer_past_its_time_stops_it_and_a_caller_waits_no_longer_than_it_may
  a_package_cant_say_it_was_stopped_or_started_again
  a_package_reaches_only_what_its_manifest_names
  a_plugin_names_its_own_start
  a_plugins_button_does_what_it_says
  no_plugins_setting_or_button_is_written_into_the_app
  a_core_sees_only_the_devices_its_manifest_names
  a_core_of_its_own_reaches_its_device_and_its_page
  a_package_speaking_another_protocol_isnt_started
  a_plugin_that_stops_says_so_until_it_is_back
  readings_are_found_by_what_they_are_from_any_plugin
  the_screen_asks_only_for_what_it_shows_and_rests_from_every_plugin
  a_plugins_reading_goes_on_the_screen
  a_keyboard_of_its_own_gets_its_page_from_its_description
  a_recipe_holds_only_what_its_plugins_manifest_names
  an_update_that_can_do_more_waits_and_a_package_must_match_its_checksum
  a_waiting_version_offers_review
)
out=$(cargo test --release -p ectomatter --bin ectomatter -- --include-ignored boundar "${runs[@]}" 2>&1; cargo test --release -p ectomatter --test stranger -- --include-ignored 2>&1)
lines=$(printf '%s\n' "$out" | grep -E '^test .* \.\.\. (ok|FAILED)$' | sort)
if [ -z "$lines" ]; then printf '%s\n' "$out" | tail -30; exit 2; fi
held=$(printf '%s\n' "$lines" | grep -c ' ok$')
not=$(printf '%s\n' "$lines" | grep -c ' FAILED$')
printf '%s\n' "$lines" | sed -E 's/^test (.*) \.\.\. ok$/  holds      \1/; s/^test (.*) \.\.\. FAILED$/  not yet    \1/'
echo "$held hold, $not don't yet"
# (a boundary test renamed or gone is a boundary no longer tried)
for t in "${runs[@]}"; do
  printf '%s\n' "$lines" | grep -q "::$t \| $t " || echo "  missing    $t (named here, not run)"
done
