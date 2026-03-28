# /// script
# requires-python = ">=3.10"
# dependencies = ["mstarpy"]
# ///
"""Fetch top holdings for a fund/ETF via mstarpy and output JSON to stdout."""

import json
import sys

import mstarpy


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

        if not holdings:
            print(json.dumps([]))
            return

        result = []
        for h in holdings:
            result.append(
                {
                    "ticker": h.get("ticker", ""),
                    "name": h.get("securityName", ""),
                    "weighting": float(h.get("weighting", 0)),
                }
            )

        print(json.dumps(result))
    except Exception as e:
        print(
            f"Error fetching holdings for '{identifier}': {e}",
            file=sys.stderr,
        )
        sys.exit(1)


if __name__ == "__main__":
    main()
