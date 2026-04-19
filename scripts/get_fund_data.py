# /// script
# requires-python = ">=3.10"
# dependencies = ["httpx>=0.27"]
# ///
"""Fetch fund metadata + holdings via Morningstar sal-service and output JSON to stdout."""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from morningstar_client import fetch_fund_data


def main():
    if len(sys.argv) not in (2, 3):
        print(
            "Usage: get_fund_data.py <morningstar_code> [limit]",
            file=sys.stderr,
        )
        sys.exit(1)

    code = sys.argv[1]
    limit = int(sys.argv[2]) if len(sys.argv) == 3 else 200

    try:
        data = fetch_fund_data(code, num=limit)
        print(json.dumps(data))
    except Exception as e:
        print(
            f"Error fetching fund data for '{code}': {e}",
            file=sys.stderr,
        )
        sys.exit(1)


if __name__ == "__main__":
    main()
