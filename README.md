Gelsemium Music Player
----------------------

Gelsemium is a free & open-source GUI and CLI music player for Linux. It is still strongly a WIP, and I make no promises of stability between updates; however, it is usable. It also supports Windows, but is tested far less thoroughly, so some things may not work.

Features 
--------
* Gapless playback
* Queue additional songs while a playlist is playing
* Supports MP3, OGG, WAV, and FLAC files
* Fast & uses little memory

Planned Features
----------------
* Playlist Editor
* Visual Customizability; customize all UI colors
* Display album art
* Visual & performance touch-ups

Usage Notes
-----------
There is no need to install Gelsemium; simply download & run the executable.  
To play in GUI mode, run the executable directly or do ./gelsemium in a terminal. To play in CLI mode, run "gelsemium play My Playlist", replacing "My Playlist" with your desired playlist. 

File format notes:  
Each playlist composes of a name in square brackets, and a list of folder or file paths (each on a separate line) composing the playlist data. The file can have any number of playlists. Filepaths are absolute. All playlist names, folders, and filenames must be valid UTF-8. On Windows, Gelsemium files are stored in [username]/Program Files/Local/GelsemiumMusicPlayer. On Linux, they're stored in /home/[username]/.local/share/GelsemiumMusicPlayer. This directory may be changed by setting the GELSEMIUM_MUSIC_PLAYER_DIR environment variable (must be valid UTF-8).  
Example:  
[Playlist Name]  
/path/to/folder  
/path/to/song  

[Another Playlist]  
/path/to/folder  
/path/to/another/folder  

[Library]  
/path/to/music/folder  