# /// script
# requires-python = ">=3.10"
# dependencies = ["httpx>=0.27"]
# ///
"""Fetch top holdings for a fund/ETF via Morningstar sal-service and output JSON to stdout."""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from morningstar_client import fetch_holdings


def main():
    if len(sys.argv) != 2:
        print(
            "Usage: get_fund_holdings.py <morningstar_code>",
            file=sys.stderr,
        )
        sys.exit(1)

    code = sys.argv[1]

    try:
        holdings = fetch_holdings(code)
        print(json.dumps(holdings[:30]))
    except Exception as e:
        print(
            f"Error fetching holdings for '{code}': {e}",
            file=sys.stderr,
        )
        sys.exit(1)


if __name__ == "__main__":
    main()
