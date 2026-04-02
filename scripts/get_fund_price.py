# /// script
# requires-python = ">=3.10"
# dependencies = ["mstarpy>=9.0.3", "python-dotenv>=1.0"]
# ///
"""Fetch the last NAV for a fund/ETF via mstarpy and output JSON to stdout."""

import json
import sys
from datetime import datetime, timedelta

from dotenv import load_dotenv

load_dotenv()

import mstarpy


def main():
    if len(sys.argv) != 2:
        print("Usage: get_fund_price.py <identifier>", file=sys.stderr)
        sys.exit(1)

    identifier = sys.argv[1]

    try:
        fund = mstarpy.Funds(identifier)
        print(fund.name)
        print(fund.isin)
        end_date = datetime.now()
        start_date = end_date - timedelta(days=10)
        history = fund.nav(start_date=start_date, end_date=end_date)

        if not history:
            print(f"No NAV data found for '{identifier}'", file=sys.stderr)
            sys.exit(1)

        last_entry = history[-1]
        price = float(last_entry["totalReturn"])
        date = last_entry["date"]

        print(json.dumps({"price": price, "date": date}))
    except Exception as e:
        print(f"Error fetching fund price for '{identifier}': {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
