# /// script
# requires-python = ">=3.10"
# dependencies = ["mstarpy>=9.0.3", "python-dotenv>=1.0"]
# ///
"""Fetch top holdings for a fund/ETF via mstarpy and output JSON to stdout."""

import json
import sys

import mstarpy
from dotenv import load_dotenv

load_dotenv()


def main():
    if len(sys.argv) != 2:
        print(
            "Usage: get_fund_holdings.py <identifier>",
            file=sys.stderr,
        )
        sys.exit(1)

    identifier = sys.argv[1]

    try:
        fund = mstarpy.Funds(identifier)
        holdings = fund.holdings()

        if holdings.empty:
            print(json.dumps([]))
            return

        top = holdings[["securityName", "weighting"]].head(30)
        result = top.to_dict(orient="records")

        print(json.dumps(result))
    except Exception as e:
        print(
            f"Error fetching holdings for '{identifier}': {e}",
            file=sys.stderr,
        )
        sys.exit(1)


if __name__ == "__main__":
    main()
