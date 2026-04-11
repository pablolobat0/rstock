# /// script
# requires-python = ">=3.10"
# dependencies = ["httpx>=0.27"]
# ///
"""Fetch the last NAV for a fund/ETF via Morningstar chartservice and output JSON to stdout."""

import json
import sys
from datetime import date, timedelta
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from morningstar_client import fetch_timeseries


def main():
    if len(sys.argv) != 2:
        print("Usage: get_fund_price.py <morningstar_code>", file=sys.stderr)
        sys.exit(1)

    code = sys.argv[1]

    try:
        end = date.today()
        start = end - timedelta(days=10)
        result = fetch_timeseries(code, start.isoformat(), end.isoformat())
        if not result:
            print(f"No NAV data found for '{code}'", file=sys.stderr)
            sys.exit(1)
        last = result[-1]
        print(json.dumps({"price": last["price"], "date": last["date"]}))
    except Exception as e:
        print(f"Error fetching fund price for '{code}': {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
