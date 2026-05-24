# Add Fund Candidate Correlation To Analyze Fund

Status: ready-for-agent

## Parent

`.scratch/fund-analysis-comparison/PRD.md`

## What to build

Extend `analyze fund` with a `--period` flag and a final **Fund candidate correlation** section. The section should compare the candidate fund against whole portfolio **NAV** and each currently held **Tracked asset**, using full requested-period coverage rules and non-fatal handling when portfolio rebuild or correlation data is unavailable.

## Acceptance criteria

- [ ] `analyze fund --code <code> --period <period>` parses and dispatches successfully.
- [ ] `--period` accepts `30d`, `6m`, `1y`, `3y`, and `5y`, and defaults to `1y`.
- [ ] Existing fund performance metrics remain YTD, 1Y, 3Y, 5Y, and all time; the new period flag affects only candidate correlations.
- [ ] Fund analysis attempts a portfolio rebuild before calculating candidate correlations.
- [ ] Portfolio rebuild or portfolio-correlation failure does not fail the fund report; unavailable rows show `N/A` with a short reason.
- [ ] Whole-portfolio correlation uses portfolio **NAV** returns, not total portfolio value.
- [ ] Held-asset correlations use **Base currency** price returns, not market value returns.
- [ ] Held-asset rows include only assets currently held in the latest portfolio snapshot.
- [ ] Correlations use aligned daily log returns and display decimal coefficients.
- [ ] Candidate correlation rows require full requested-period coverage with seven-day start/end tolerance.
- [ ] Candidate correlations use the latest portfolio **NAV** date as the requested end date.
- [ ] Output appears at the end of `analyze fund`, with portfolio **NAV** first, then held assets sorted by descending correlation, and unavailable rows last.

## Blocked by

- `.scratch/fund-analysis-comparison/issues/01-add-fund-quote-metadata-to-single-fund-analysis.md`
