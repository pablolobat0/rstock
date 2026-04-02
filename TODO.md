# rstock — TODO

## Bugs / Code Quality

- [ ] Reject buy/sell with quantity <= 0 — currently accepts zero-quantity transactions
- [ ] Inspect why stock prices are not in real time

## New Commands

- [ ] `edit` — edit a transaction by ID (with confirmation prompt)
- [ ] `delete` — remove a transaction by ID (with confirmation prompt)
- [ ] `import` — bulk import transactions from broker CSV exports

## Portfolio Features

- [ ] Benchmark comparison chart — overlay portfolio NAV against benchmark (ACWI) on same chart
- [ ] Sector/country/asset type allocation breakdown
- [ ] Rebalancing alerts — target allocation vs. current weights

## Structural Improvements

- [ ] Add structured logging — replace `eprintln!()` warnings
- [ ] Config file (`~/.rstock/config.toml`) — default currency, DB path, chart period, etc.
- [ ] Database backup/restore command

## Monitor Improvements

- [ ] Add color to monitor performance graph — distinguish stock vs sector ETF lines with color
- [ ] Redesign monitor display layout — improve readability and visual presentation
