The latest patch of the v0.10.X line with some features to help keep squad owners sane, especially those with large squads.
Other than that, just a bunch of fixes here and there.
Let us know what you think in our Discord!

## Features
* Add ability to bind a secondary push-to-talk button.
* Add support for the latest Valorant patch (Chamber).
* Add support for the latest TFT set.
* All filters are now saved after each change for each page so that the next time you reload the app, the same filter settings are used.
* Add ability to change whether or not all your past VODs/clips are shared to a squad after you join (disabled by default).
* Add the ability to display discoverable, public squads that users can join.
* Add ability to blacklist users from sharing content to a squad you own.
* Add ability to remove (un-share) vods/clips that have been shared to a squad you own.
* Add ability for squad owners to choose what types of content can be shared to your squad (by games and by game-mode for WoW).

## Removed
* Removed the WoW-specific option to record full raids as it was not working properly for the vast majority of users.

## Improvements
* Made the post-game report more prominent after you exit the game.
* An error now pops up if the overlay preview fails to start.
* Separate out the option to record WoW dungeons from WoW keystones.
* Separate out the option to record WoW battlegrounds from WoW arenas.
* Improved the wording on the auto-sharing settings page.
* We now paginate the user list on the home page for large squads.
* Added another dialog to ask what you want to do after you create a clip to better support users who want to create multiple clips from a single VOD.
* Improved the speed of some database queries for pulling recent VODs, clips, and profile page VODs.

## Bug Fixes
* Fixed an issue where the register button may show up for a split second when refreshing when already logged in.
* Fixed an issue where users may get stuck on generating the settings file.
* Fixed an issue where the navigation bar would show up on pages where they shouldn't (e.g. the vod editor, oembeds, etc.).
* Fixed an issue where the clip library button for each match disappeared.
* Fixed an issue where we wouldn't be parsing Hearthstone logs properly if the time format was not formatted in the US locale.
* Reverted the change to make GDI the default window capture as it didn't work properly across the majority of the games we support - WGC (yellow border) is the default again.
* Fixed the issue where WoW classic spells would not show up in the timeline properly.
* Fixed an issue where extra spaces at the ends of usernames/passwords in the login/registration screen would not be removed.
* Fixed an issue where player status while playing WoW classic is not transmitted.
* Fixed an issue where we would try to use GDI recording for the overlay when WGC must be used for the game.
* Fixed an issue where we would not cleanup created clips from users' machines.
* Fixed an issue where extra players would show up in returned player list for WoW classic.
* Fixed an issue where WoW clips would not be returned in the clip library.
* Fixed an issue where a duplicate player may be returned in the player list for a WoW match.