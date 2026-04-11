# /// script
# requires-python = ">=3.10"
# dependencies = ["httpx>=0.27"]
# ///
"""Fetch historical NAV for a fund/ETF via Morningstar chartservice and output JSON to stdout."""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from morningstar_client import fetch_timeseries


def main():
    if len(sys.argv) != 4:
        print(
            "Usage: get_fund_price_history.py <morningstar_code> <start_date> <end_date>",
            file=sys.stderr,
        )
        sys.exit(1)

    code = sys.argv[1]
    start_date = sys.argv[2]
    end_date = sys.argv[3]

    try:
        result = fetch_timeseries(code, start_date, end_date)
        if not result:
            print(f"No NAV data found for '{code}'", file=sys.stderr)
            sys.exit(1)
        print(json.dumps(result))
    except Exception as e:
        print(
            f"Error fetching fund price history for '{code}': {e}",
            file=sys.stderr,
        )
        sys.exit(1)


if __name__ == "__main__":
    main()
