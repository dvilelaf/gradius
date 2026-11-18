# Gradius for PC

An unofficial PC version of **Gradius**, the classic NES space shooter, for Windows and Linux.

**The game itself is not included.** You need your own copy of *Gradius* for the NES, European version, as a `.nes` file.

## How to play

1. Go to the [latest release](https://github.com/dvilelaf/gradius/releases/latest) and download the file for your computer:
   - **Windows:** `Gradius-0.1.0-windows-x86_64.zip`
   - **Linux:** `Gradius-0.1.0-x86_64.AppImage`
2. Open it:
   - **Windows:** right-click the zip, choose **Extract All**, open the new folder and double-click `gradius.exe`. If Windows shows a blue warning, click **More info**, then **Run anyway**.
   - **Linux:** right-click the file, open **Properties**, turn on **Allow executing as a program**, then double-click it.
3. Drag your `.nes` file onto the game window, or press Enter to look for it.
4. Wait a moment while the game gets ready. You only do this once.

## Controls

| | Keyboard | Gamepad |
| --- | --- | --- |
| Move | Arrow keys | D-pad or left stick |
| Fire | Z or Space | A |
| Use power-up | X | B |
| Start / pause | Enter | Start |
| Options | Esc | Back or Guide |
| Full screen | F11 | |
| Save / load | F5 / F9 | |

On the title screen, Up and Down choose 1 or 2 players. Player 2 can use a second gamepad, or T F G H to move, R to fire, Y for power-ups and 1 for Start.

## Options

Press Esc during the game to change:

- **Start stage:** begin at any of the 7 stages.
- **Ship hits:** how many hits your ship takes, from 1 to 99, or **Invincible**.
- **2 players at once:** both ships on screen together.
- **Stage 5 slowdown:** the original game slows down in stage 5. This version doesn't, unless you turn this on.
- **Volume** and **Fullscreen**.

Everything except volume and fullscreen applies from your next game.

## Starting over

To give the game a different `.nes` file, close it and delete `gradius.pak`:

- **Windows:** it is in the same folder as `gradius.exe`.
- **Linux:** it is in `~/.local/share/gradius`.

## More

Building from source and technical notes are in [docs/TECHNICAL.md](docs/TECHNICAL.md).

Code licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE). Gradius and Konami are trademarks of Konami Digital Entertainment. This project is not affiliated with or endorsed by Konami, and it contains no Konami code, art or music.
