Websocket cap
Rate limiting
game pruning
tier-based game-pruning
games-per-ip limit
privacy notice
players can technically move other color. This is bad

LATER:
Currently, the way moves and events work is all messed up and extremely prone to desyncs. This is only
mitigated by the fact that chess itself is very resilient against desyncs. This thing, however, effectively
causes client synchronization to look odd and forces the client to sometimes ignore events and sometimes not.

For future reference, moves should be resolved via requests coming in from the server, responses going from the
client, the client pushing its own state forward, and awaiting a confirmation (move event) from the server. This
is how grownups do it.
