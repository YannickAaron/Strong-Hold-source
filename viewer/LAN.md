# How to play over the local network (LAN)

Two Macs on the same Wi-Fi / network, or two copies of the app on one Mac for testing.

1. **Every Mac needs** the same build of this app and the game installed in CrossOver (the maps are read from there).
2. **Host:** Main menu → **Multiplayer** → **Host a game**. The lobby opens (same screen as Skirmish). Pick the map, add
   AI lords, teams, options. Wait until the other players' names show up in the player list.
3. **Other players:** Main menu → **Multiplayer**. The host's game shows up under "Games on your network" within a few
   seconds — click it. Not there? Type the host's address under **Or join by address** (e.g. `192.168.1.20`) → **Join**.
   You see the host's lobby; only the host can change it.
4. **Host clicks Play** (the hand bottom right). All games load and start together.

Save and load: save as usual (⌘S, Save Game or F5; in a LAN game the game keeps running meanwhile). A LAN game saves as
a multiplayer save (`.msv`, like the original). Later the host opens a lobby with the same number of players, clicks
**Load** (the key icon) in the lobby, which lists only multiplayer saves, picks one, then Play. Players take the save's
human castles in order.

During the game
- **Chat:** press Return (or keypad Enter), type, Return sends it to everyone; Escape closes the line. The last 5
  messages show at the top left for 20 s.
- **Pause:** P pauses the game for everyone ("Game Paused - <name>"); P again (any player) resumes. Each player can
  pause 3 times per game. (The original has no pause in multiplayer; this is ours.)
- The game runs at the host's speed; if a player is slower, all wait.
- The game adapts its input delay to the network: on a good network orders take effect after ~0.1–0.2 s.
- If the other player is slow, your game waits: "Communication delay. Waiting for other players." (and the window
  title says "waiting for <name>").
- **A player drops out:** everyone waits up to 30 s ("waiting for player N to come back"). They rejoin from
  Multiplayer — the game shows **rejoin**. Not back in time: their castle is removed and the rest play on.
- **The host quits:** after a few seconds another player becomes the host and the game goes on.
- **Watch a game:** a running game shows **watch** in the Multiplayer list; click it to spectate.
- **Desync** (the games no longer match — a bug): the host repairs it automatically (window title "resync after …",
  a short pause). If the repair fails the game stops with "Desync at tick …": please report the message.

If the game does not show up
- macOS may ask "accept incoming network connections?" for the app — click Allow (on every Mac).
- Ports: UDP 47777 (finding games) and TCP 47778 (the game). All Macs must be on the same network (no guest Wi-Fi).
- Two copies on one Mac: start the host first; the second copy finds it the same way.
