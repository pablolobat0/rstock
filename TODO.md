# rstock — TODO

## Bugs / Code Quality

- [x] Reject buy/sell with quantity <= 0 — currently accepts zero-quantity transactions
- [x] Inspect why stock prices are not in real time


## Structural Improvements

- [x] Add structured logging — replace `eprintln!()` warnings

## Code Review

- [ ] Review and improve project structure
- [ ] Review and improve tests — coverage, quality, correctness
- [ ] Review and improve code quality and comments
- [x] Review CLAUDE.md — reduce token usage while keeping essential info
- [ ] Improve and extend analysis feature
- [x] Improve and change CLI structure
- [x] Review metrics calculus, adapt method to not use models of the code, just enter numbers. Move database access out of metrics service. Chech that trading days are the same as returns


## Display
- [x] Use , for decimals separator and . for the others
- [ ] Improve graphs
- [x] Review green and red color in tables

## Monitor Improvements

- [ ] Add color to monitor performance graph — distinguish stock vs sector ETF lines with color
- [ ] Redesign monitor display layout — improve readability and visual presentation


## New Commands

- [x] `edit` — edit a transaction by ID (with confirmation prompt)
- [x] `delete` — remove a transaction by ID (with confirmation prompt)
- [x] `import` — bulk import transactions from broker CSV exports

## Analysis Features

- [ ] Sortino ratio — downside deviation risk metric
- [ ] Rolling correlations — time-varying correlation windows
- [ ] Fama-French factors — multi-factor model exposure analysis

## Portfolio Features

- [ ] Benchmark comparison chart — overlay portfolio NAV against benchmark (ACWI) on same chart
- [ ] Sector/country/asset type allocation breakdown
- [ ] Rebalancing alerts — target allocation vs. current weights
