# Color Lines mechanics

All values below are taken directly from the request unless marked estimated. Classic Lines details left open by the request are estimates.

| Value | Setting | Source |
| --- | --- | --- |
| Board | 9 × 9 cells | Request |
| Ball colors | 7 | Request |
| Balls at game start | 5 | Estimated from classic Lines |
| Balls added after a non-scoring move | 3 | Request |
| Upcoming-ball preview | 3 colors | Request |
| Line length needed to clear | 5 or more | Request |
| Line directions | Horizontal, vertical, both diagonals | Request |
| Movement | Orthogonal steps through empty cells, shortest path found with BFS | Request; orthogonal steps are classic Lines behavior |
| Scoring | 2 points per cleared ball; overlapping lines count a ball once | Estimated |
| Scoring move | No new balls spawn after a move that clears | Request |
| Spawned-ball lines | Cleared immediately, using the same scoring rule | Estimated from classic Lines |
| Game over | Board has no empty cells after clearing is resolved | Request |
| Ball movement animation | 75 ms per cell | Estimated |
| Controls | Click a ball, then an empty cell; click another ball to change selection | Request |
| Interface language | English | Request |
