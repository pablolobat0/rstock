# Add Compare Funds Correlation And Aligned Return Graph

Status: done

## Parent

`.scratch/fund-analysis-comparison/PRD.md`

## What to build

Extend `compare funds` with selected-period fund-to-fund correlation and an aligned return graph. The selected period controls only this correlation/graph section. The graph should align both funds to `0%` at the first shared price in the selected period and should not fall back to shorter history when full requested-period coverage is unavailable.

## Acceptance criteria

- [x] The comparison period controls fund-to-fund correlation and the aligned return graph, not the multi-period performance table.
- [x] Fund-to-fund correlation uses aligned daily log returns.
- [x] Graph cumulative returns use price-relative returns from the first shared price in the selected period.
- [x] Full selected-period coverage is required for both funds, with seven-day start/end tolerance for normal calendar/source cadence.
- [x] If full coverage is unavailable, the section shows `N/A` with a short reason and does not render a fallback graph.
- [x] The section title includes the selected period.
- [x] The correlation value appears above the graph.
- [x] The graph is an overlaid ASCII terminal chart for both funds.
- [x] The graph includes a legend and start/end return summaries.
- [x] The correlation/graph section appears near the end of the comparison output.

## Blocked by

None - can start immediately
