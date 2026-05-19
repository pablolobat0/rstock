# Infer FX Inside Historical Market Data Preparation

Status: done

Type: AFK

## Parent

`.scratch/deepen-historical-market-data/PRD.md`

## What to build

Change Historical market data preparation so callers supply required assets and the Module infers required FX from those assets. NAV and benchmark preparation should no longer require callers to pass provider-specific FX identities, while Effective valuation date behaviour remains unchanged.

## Acceptance criteria

- [ ] Historical market data preparation derives required FX for every non-Base currency asset.
- [ ] Base currency assets use implicit FX rate 1.0 and do not require FX Historical market data.
- [ ] NAV preparation callers no longer build or pass FX pair strings.
- [ ] Benchmark preparation callers no longer build or pass FX pair strings.
- [ ] Effective valuation date remains the minimum supported date across requested date, required asset prices, and required FX rates.
- [ ] Tests cover FX inference, Base currency implicit FX, and Effective valuation date limiting from inferred FX.

## Blocked by

None - can start immediately
