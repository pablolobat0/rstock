# Immutable Historical Market Data During Routine Commands

rstock treats successful Historical market data for completed dates as immutable during routine portfolio and analysis commands. Market data preparation reads persisted observations, requests only missing intervals, shares identical requests and results within one command, and lets every later command retry failed source requests; this favors reproducible NAV history and lower source/database work over automatically incorporating later source corrections.

## Consequences

Routine commands never replace successful persisted observations, and this initiative does not add a refresh or rebuild command. Correcting an inaccurate cached observation therefore requires manual database intervention until a separately designed explicit correction operation exists.
