# Add Compare Funds Correlation And Aligned Return Graph

Status: ready-for-agent

## Parent

`.scratch/fund-analysis-comparison/PRD.md`

## What to build

Extend `compare funds` with selected-period fund-to-fund correlation and an aligned return graph. The selected period controls only this correlation/graph section. The graph should align both funds to `0%` at the first shared price in the selected period and should not fall back to shorter history when full requested-period coverage is unavailable.

## Acceptance criteria

- [ ] The comparison period controls fund-to-fund correlation and the aligned return graph, not the multi-period performance table.
- [ ] Fund-to-fund correlation uses aligned daily log returns.
- [ ] Graph cumulative returns use price-relative returns from the first shared price in the selected period.
- [ ] Full selected-period coverage is required for both funds, with seven-day start/end tolerance for normal calendar/source cadence.
- [ ] If full coverage is unavailable, the section shows `N/A` with a short reason and does not render a fallback graph.
- [ ] The section title includes the selected period.
- [ ] The correlation value appears above the graph.
- [ ] The graph is an overlaid ASCII terminal chart for both funds.
- [ ] The graph includes a legend and start/end return summaries.
- [ ] The correlation/graph section appears near the end of the comparison output.

## Blocked by

None - can start immediately
