# /// script
# requires-python = ">=3.10"
# dependencies = ["mstarpy"]
# ///
"""Fetch historical NAV for a fund/ETF via mstarpy and output JSON to stdout."""

import json
import sys
from datetime import datetime

import mstarpy


def main():
    if len(sys.argv) != 4:
        print(
            "Usage: get_fund_price_history.py <identifier> <start_date> <end_date>",
            file=sys.stderr,
        )
        sys.exit(1)

    identifier = sys.argv[1]
    start_date = sys.argv[2]
    end_date = sys.argv[3]

    try:
        fund = mstarpy.Funds(identifier)
        start_dt = datetime.strptime(start_date, "%Y-%m-%d")
        end_dt = datetime.strptime(end_date, "%Y-%m-%d")
        history = fund.nav(start_date=start_dt, end_date=end_dt)

        if not history:
            print(f"No NAV data found for '{identifier}'", file=sys.stderr)
            sys.exit(1)

        result = []
        for entry in history:
            result.append({"date": entry["date"], "price": float(entry["nav"])})

        print(json.dumps(result))
    except Exception as e:
        print(
            f"Error fetching fund price history for '{identifier}': {e}",
            file=sys.stderr,
        )
        sys.exit(1)


if __name__ == "__main__":
    main()
