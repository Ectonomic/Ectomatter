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
# stranger's computer has it (tests/stranger.rs), and its pages drawn on a
# made-up desk, as approved (tests/renders.rs), and installed as
# tools/install.sh lays it out (tests/install.rs), and as its RPM holds it
# (tests/rpm.rs); and started with no compositor, as at login before KWin is
# ready, and with KWin gone and back (tests/compositor.rs); and opened again
# while it runs, on another virtual desktop (tests/once.rs); and where it keeps
# starting at login, as each system has it (autostart.rs); and every library
# at its newest release (tools/fresh.py) with no security advisory
# (tools/security.sh)
runs=(
  a_package_past_its_time_is_stopped_and_started_again
  a_package_out_of_memory_or_crashed_is_stopped_and_started_again
  a_timer_past_its_time_stops_it_and_a_caller_waits_no_longer_than_it_may
  a_package_cant_say_it_was_stopped_or_started_again
  a_package_reaches_only_what_its_manifest_names
  a_plugin_names_its_own_start
  a_plugins_button_does_what_it_says
  no_plugins_setting_or_button_is_written_into_the_app
  no_cores_setting_or_slider_is_written_into_the_app
  a_core_sees_only_the_devices_its_manifest_names
  a_core_of_its_own_reaches_its_device_and_its_page
  a_package_speaking_another_protocol_isnt_started
  a_plugin_that_stops_says_so_until_it_is_back
  readings_are_found_by_what_they_are_from_any_plugin
  the_screen_asks_only_for_what_it_shows_and_rests_from_every_plugin
  a_plugins_reading_goes_on_the_screen
  a_keyboard_of_its_own_gets_its_page_from_its_description
  a_keyboard_of_its_own_gets_its_sliders
  a_recipe_holds_only_what_its_plugins_manifest_names
  an_update_that_can_do_more_waits_and_a_package_must_match_its_checksum
  a_waiting_version_offers_review
  each_systems_entry_starts_the_program_it_names
  starting_at_login_is_written_where_the_desktop_reads_it
  every_package_is_added_from_its_release_on_github
)
out=$(cargo test --release -p ectomatter --bin ectomatter -- --include-ignored boundar "${runs[@]}" 2>&1; cargo test --release -p ectomatter --test stranger -- --include-ignored 2>&1; cargo test --release -p ectomatter --test renders 2>&1; cargo test --release -p ectomatter --test install 2>&1; cargo test --release -p ectomatter --test rpm 2>&1; cargo test --release -p ectomatter --test compositor 2>&1; cargo test --release -p ectomatter --test once 2>&1)
lines=$(printf '%s\n' "$out" | grep -E '^test .* \.\.\. (ok|FAILED)$' | sort)
if [ -z "$lines" ]; then printf '%s\n' "$out" | tail -30; exit 2; fi
held=$(printf '%s\n' "$lines" | grep -c ' ok$')
not=$(printf '%s\n' "$lines" | grep -c ' FAILED$')
printf '%s\n' "$lines" | sed -E 's/^test (.*) \.\.\. ok$/  holds      \1/; s/^test (.*) \.\.\. FAILED$/  not yet    \1/'
# (every dependency that ships, and every file carried from others, under a
# license deny.toml approves:
# tools/licenses.sh; a crate docs/deferred.md names is a known failure, any
# other stops the run)
licenses=$(tools/licenses.sh); case $? in
  0) echo "  holds      every_shipped_dependencys_license_is_approved"; held=$((held + 1)) ;;
  3) echo "  not yet    every_shipped_dependencys_license_is_approved (known failures, docs/deferred.md)"; not=$((not + 1)) ;;
  *) printf '%s\n' "$licenses"; echo "  FAILS      every_shipped_dependencys_license_is_approved: a crate or carried file not approved and not in docs/deferred.md, or a carried file not in REUSE.toml"; exit 1 ;;
esac
# (every library Ectomatter and its packages name themselves at its newest
# release, and every one under them at the newest their own needs allow:
# tools/fresh.py; a library docs/deferred.md names is a known failure)
fresh=$(python3 -I tools/fresh.py); case $? in
  0) echo "  holds      every_library_is_at_its_newest_release"; held=$((held + 1)) ;;
  3) printf '%s\n' "$fresh" | grep -v ' holds '; echo "  not yet    every_library_is_at_its_newest_release (known failures, docs/deferred.md)"; not=$((not + 1)) ;;
  *) printf '%s\n' "$fresh"; echo "  FAILS      every_library_is_at_its_newest_release: a library behind and not in docs/deferred.md, or crates.io not reached"; exit 1 ;;
esac
# (no library that ships with a known security hole, unsound or no longer
# looked after, and every one from crates.io, as RustSec's advisories have
# it: tools/security.sh; an advisory docs/deferred.md names is a known failure)
security=$(tools/security.sh); case $? in
  0) echo "  holds      no_library_has_an_advisory"; held=$((held + 1)) ;;
  3) printf '%s\n' "$security" | grep -v ' holds$'; echo "  not yet    no_library_has_an_advisory (known failures, docs/deferred.md)"; not=$((not + 1)) ;;
  *) printf '%s\n' "$security"; echo "  FAILS      no_library_has_an_advisory: an advisory not in docs/deferred.md, a library from elsewhere, or the database not reached"; exit 1 ;;
esac
# (the list of packages, registry/packages.json, held to the releases on
# GitHub by the check every change to the list must pass, registry/check.py;
# and every package that comes with Ectomatter as it stands here held to its
# release's source, tools/released.py: one changed since, without a new
# version, fails)
if listed=$(python3 -I registry/check.py) && released=$(python3 -I tools/released.py); then
  echo "  holds      the_list_of_packages_is_as_they_are_released"; held=$((held + 1))
else
  printf '%s\n' "$listed" "${released:-}"; echo "  FAILS      the_list_of_packages_is_as_they_are_released: a release isn't what the list pins, or a package changed here since its release (give it a new version: tools/package-repos.py, tools/list-packages.py)"; exit 1
fi
echo "$held hold, $not don't yet"
# (a boundary test renamed or gone is a boundary no longer tried)
for t in "${runs[@]}"; do
  printf '%s\n' "$lines" | grep -q "::$t \| $t " || echo "  missing    $t (named here, not run)"
done
# (none of the words kept outside the repository, a person's own details, in
# its files: tools/private-words.sh)
tools/private-words.sh || exit 1
