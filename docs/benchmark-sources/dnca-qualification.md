# DNCA Alpha Bonds — €STR + 1.20% objective qualification

**Evidence checked:** 20 October 2026. **Status: the exact class and 120 bp spread are verified; the spread accrual, day count, compounding, observation convention and pre-launch construction are unresolved. The objective is not enabled.**

The approved target definition remains
`estr-plus-120bp`, the **€STR + 1.20%** objective of the exact
**B (EUR)** class of DNCA Invest Alpha Bonds, ISIN **LU1694789535**.
Plain €STR remains a separate catalog target and a separate comparison; it is not
the objective and is not substituted for it. This note records what dated primary
evidence establishes, what it does not, and why no objective series is admitted.
It does not approve a substitute cash rate, another class's spread, a pre-2019
predecessor splice, a shorter comparison window, or an application-side formula
chosen without a gate decision. The absence of a formula in the retrieved public
documents is a qualification blocker, not a finding that DNCA or its index
provider has no internal methodology.

## Outcome

The exact class, the current **1.20%** spread and the class-specific objective are
verified. The documents below also establish that the 2018 and 2019 reference
returns shown in the current report are **not literal €STR + 1.20% history**: the
December 2019 B-class report names **EONIA Capitalisé + 250BP** and publishes the
same 2.20% and 2.16% reference returns that the current report relabels as
€STR + 1.20%. The exact date of the benchmark change and the construction of any
predecessor remain unverified.

The current benchmark is **not** a bond-market index and **not** an investment
universe; DNCA uses it for the performance fee and for performance comparison.
No DNCA document retrieved states how the 1.20% is accrued, which day count or
compounding convention applies, whether the spread is added to the overnight rate
before compounding or to the compounded return, or how weekends, TARGET2 holidays,
reference dates and publication dates are treated. The prospectus only names
`ESTRON Index` as the "ticker of the composite"; it does not publish a formula or a
public composite-series identity.

The ECB separately publishes a fully documented **plain €STR compounded index**:
ISIN `EU000A2QQF08`, starting at 100 on 1 October 2019, calculated on an ACT/360
basis over TARGET2 business days and published at 09:15 without later revision.
That qualifies the plain €STR leg and its source access, but it does not establish
DNCA's spread accrual. The ECB's pre-€STR series is explicitly not a benchmark or
reference rate, so it cannot extend literal €STR history backwards.

Both catalog entry points stay **target-only**. Dependent fund-relative results
must stay unavailable with a precise limitation; fund NAV, holdings, inventory and
portfolio-only facts remain unaffected. No source adapter, seed, schema, CLI,
assignment or cached observation is changed by this evidence.

## Findings by gate

| Gate | Evidence and conclusion | Status |
| --- | --- | --- |
| Exact class and vehicle identity | The PRIIPs KID (production date **16/04/2026**, p. 1) names **DNCA Invest ALPHA BONDS**, share class **B (EUR)**, ISIN **LU1694789535**, manufacturer DNCA Finance. The B-class monthly report (data **30/09/2026**, p. 1 and p. 5) records inception **14/12/2017**, accumulation, currency EUR and daily valuation. The 31/12/2025 annual report (p. 247) lists the same B shares EUR class. | **Verified** |
| Current objective of that exact class | The KID p. 1 states the product seeks "a higher performance, net of any fees, than the €STR index plus 1.20%". The annual report p. 247 lists **B shares EUR: €STER + 1.20%**. The B-class monthly report (30/09/2026, p. 1 and p. 5) labels the reference index **€STR + 1.20%**. The KID also uses the same hurdle for the 20% performance fee with a high-water mark. | **Verified** |
| Objective role | The documents call it the reference index for performance comparison and the performance-fee hurdle. It is an **Objective comparator**, not an investment universe, not a bond index and not a holdings replica. The fund is explicitly actively and discretionarily managed. | **Verified** |
| Exact 120 bp spread, not another class | The annual report p. 247 lists different objectives per class: A/AD **1.40%**, B **1.20%**, N/ND **1.80%**, I/ID/N-SP **2.00%**, F **2.20%**, Q **2.40%**, SI **2.10%**, WI **1.70%**, AFERG **1.80%**. The monthly reports for A and ND independently show 1.40% and 1.80%. Another class's objective spread must not be substituted for the B class. | **Verified** |
| Spread accrual | No retrieved DNCA document states whether 1.20% is added to the overnight rate before daily compounding, accrued on a simple basis, or added to the compounded return. The prospectus (20/03/2026, pp. 319–320) says the Reference Asset Value is "multiplied by the daily return of the Index"; the annual report (p. 247) says "multiplied by the Index". Neither defines the arithmetic. | **Unresolved** |
| Day count | No DNCA document states 360 or 365, actual days, or any other denominator. The ECB plain €STR compounded index uses ACT/360 and the Bloomberg cash-rate definition of `ESTRON` also lists 360, but neither is evidence that DNCA's B-class composite uses that convention. | **Unresolved** |
| Compounding | The word "Capitalisé" appears in the 2019 predecessor label (EONIA Capitalisé +250BP), and the current objective is described only as "multiplied by the daily return of the Index". That does not establish daily compounding for the current benchmark. Annual, daily and simple variants remain indistinguishable from the public documents. | **Unresolved** |
| Weekend, holiday and TARGET2 carry | No DNCA document states how the benchmark treats Saturdays, Sundays, TARGET2 holidays or a missing rate publication. The ECB plain €STR index uses 3 calendar days for Mondays and adjusts for TARGET2 holidays; applying that to the 120 bp spread is an assumption, not a verified DNCA convention. | **Unresolved** |
| Effective/reference date versus publication date | The ECB rate is published on a TARGET2 business day and reflects trading on the previous TARGET2 date. The compounded index is published at 09:15 on each TARGET2 day; the calculation rules state that, for a publication, the end/reference date coincides with the publication date, and the Data Portal series is indexed by that date. No DNCA document states whether its daily reference index return for a fund-NAV date uses the index dated that day, the preceding day, or the rate-publication date. | **Unresolved** |
| Exact composite identity and series key | The prospectus (p. 321) names **`ESTRON Index`** as the "ticker of the composite" for the B class. Bloomberg's primary Overnight Cash Indices document defines `ESTRON Index` as the **cash-rate ticker** (not a composite index) and lists 360 days/year, publication lag 1 and TARGET2 pricing days. No public composite-series identifier, term sheet or index methodology tying DNCA's +1.20% composite to that ticker was retrieved. `ESTRON Index` is therefore not accepted as the exact objective series identity. | **Unresolved** |
| Permitted access to the exact objective series | The DNCA documents are public downloads, but they do not grant a data feed. The only retrieved primary construction for an `ESTRON`-based cash-plus series is Bloomberg's methodology; actual Bloomberg index values were not accessed, the methodology document alone does not grant access, and no entitlement for this application was produced. If the objective is instead calculated by this application from ECB plain €STR, that formula is an application convention that has not been approved. | **Unresolved** |
| Historical reference change | The B-class December 2019 management report (data 31/12/2019, p. 1) states the objective was "a higher performance ... than the **EONIA index plus 2.5%**" and footnotes the reference as **EONIA Capitalisé +250BP**; it reports 2018 **+2.20%** and 2019 **+2.16%** for that reference. The current B-class report publishes exactly the same 2018 **+2.20%** and 2019 **+2.16%** values under the label **€STR + 1.20%**. Current 2024 and 2025 reference returns differ by the class spread (B +4.96% / +3.43%; A +5.16% / +3.63%), which is consistent with the current class-specific benchmark being in use by 2024. The exact change date, the construction of the intervening history and whether any predecessor was intentionally continued are not documented. | **Partially verified / change date unresolved** |
| Pre-2019 construction | €STR was first published on **2 October 2019**, reflecting trading activity on **1 October 2019**; there is no literal €STR observation before that. The current report's pre-2019 values are therefore not literal €STR + 1.20%. The ECB's pre-€STR series is explicitly "for information purposes only" and "not intended for use as a benchmark or reference rate". The only retrieved predecessor label is EONIA Capitalisé +250BP, and no dated document authorizes splicing it into the current objective. | **Unresolved; block pre-2019** |
| Plain €STR source identity and access | The ECB publishes the €STR rate (ISIN `EU000A2X2A25`) and a compounded €STR index (ISIN `EU000A2QQF08`) on each TARGET2 business day. The index starts at **100.00000000 on 1 October 2019**, uses ACT/360 and daily compounding, is published at **09:15 CET**, is final after publication and is not revised. The ECB states that it does not charge for the rate or the compounded index and does not license their use. The Data Portal series key `EST.B.EU000A2QQF08.CI` is the plain compounded index. | **Verified separately** |
| Plain €STR cadence and coverage | The compounded index is published every TARGET2 business day, not every calendar day. A sample downloaded from the ECB Data API on **20 October 2026** covers **2019-10-01 through 2026-10-20**; the series begins on 1 October 2019 and has no pre-2019 literal history. A weekday comparison must align on actual TARGET2 observations and must not invent weekend or holiday closes. | **Verified separately** |

## What the published reference returns do and do not prove

The current B-class report shows reference returns of **+4.96% for 2024** and
**+3.43% for 2025**. A calculation from the downloaded ECB compounded-index CSV
using the last TARGET2 observations of each calendar year gives approximately
**+3.80%** and **+2.24%**. Adding 1.20 percentage points gives approximately
**+5.00%** and **+3.44%**. The 2024 and 2025 results are therefore broadly
consistent with a compounded €STR series plus the B-class spread.

That is a consistency check, not a verified formula. A few basis points remain,
which is the same order as the difference between ACT/360 and ACT/365 spread
accrual, different year-end observation dates, and rounding to two decimals. The
public returns cannot distinguish:

- adding 1.20% to the overnight rate before compounding from compounding the
  rate and then adding the spread;
- daily compounding from an annual or simple accrual;
- ACT/360 from ACT/365;
- TARGET2 publication-lag 1 from the reference-date observation;
- 1 October 2019 from a later effective date for the current benchmark.

None of those choices may be inferred from the KID's target name, the ECB's
plain-€STR day count, or a rounded monthly return.

## The predecessor problem

The current report's long history is a retrospective presentation, not a
continuous literal €STR + 1.20% series:

- the B-class **December 2019** report explicitly names **EONIA Capitalisé
  +250BP** and shows **2018 +2.20%** and **2019 +2.16%** for that reference;
- the current B-class report reproduces **2018 +2.20%** and **2019 +2.16%** under
  the **€STR + 1.20%** label;
- 2024 and 2025 reference returns differ by exactly the class spread between B and
  A, while the 2018 and 2019 values are identical across the published class
  tables.

The exact change date and the construction of the predecessor are not stated in
the retrieved documents. The predecessor cannot be spliced automatically, and the
current report's label cannot be treated as proof that the 2018/2019 values are
literal €STR + 1.20%. A comparison requesting a window before the first literal
€STR observation must be **unavailable**, not shortened to the available period
and not filled with EONIA, pre-€STR, another class or another cash rate.

## Plain €STR is a distinct target

Plain €STR is qualified independently as the ECB compounded index:

| Property | Verified fact |
| --- | --- |
| Identity | €STR rate ISIN `EU000A2X2A25`; compounded index ISIN `EU000A2QQF08`; Data Portal key `EST.B.EU000A2QQF08.CI`. |
| Base and formula | 100.00000000 on 1 October 2019; compounded daily using the published €STR rate, ACT/360, with 1 calendar day on Tuesday–Friday and 3 days on Monday under normal TARGET2 conditions, adjusted for TARGET2 holidays. |
| Publication | Compounded index every TARGET2 business day at 09:15 CET; final and not revised after publication. Rate published at 08:00 CET on each TARGET2 business day. |
| Coverage | Literal series starts 1 October 2019; no pre-2019 literal observations. |
| Access | ECB states no charge and no licence for the rate or the compounded index. |
| Cadence | Daily TARGET2 business days, not calendar-daily. Weekend and holiday dates are not independent observations. |

This qualifies the **plain €STR** source and its compounding convention. It does
not qualify DNCA's **+1.20%** composite, and it does not authorize adding a spread
to the ECB index as if DNCA had documented that calculation. Plain €STR is a
separate secondary target, not a fallback for the objective and not a substitute
for the 3% risk-free metric assumption or the 0%-return portfolio dividend cash.

## Behavioural contract for the eventual implementation

When the missing convention is resolved, the shared Benchmark Module should
construct the objective wealth and the aligned primary fund comparison through the
existing public comparison path, with fake sources, a fixed clock and in-memory
SQLite in the offline suite. The implementation must preserve:

- **Identity:** the exact B-class 1.20% spread, with the class ISIN and the
  objective-comparator role visible in human and JSON output. Another class's
  spread is never substituted.
- **Accrual provenance:** the verified spread, day count, compounding, weekend and
  TARGET2-holiday treatment, observation/reference date and publication-date
  treatment, source series identity, coverage and pre-launch boundary must travel
  with the generated wealth and the comparison.
- **Date alignment:** both series must be aligned on identical completed dates.
  The objective's publication lag must not be silently shifted, and a missing
  TARGET2 observation must not become an invented calendar-day return.
- **Pre-launch boundary:** no observation before the first literal €STR date. No
  automatic EONIA or pre-€STR splice. A request that reaches earlier must block
  the dependent window honestly rather than shorten it.
- **Independent failure:** an unavailable or unverified objective makes only the
  dependent objective comparison unavailable. The fund's NAV, holdings, inventory
  and portfolio-only facts remain available. A failure must not fall back to plain
  €STR or to another class's objective spread.
- **Plain €STR:** the optional secondary is a distinct catalog target with its own
  role, source identity, cadence and provenance. It never replaces the primary
  objective.
- **Unchanged assumptions:** the retained 3% annual risk-free metric assumption and
  the 0%-return retained dividend cash are not changed by enabling or disabling
  €STR.

Offline fixtures would need to pin the verified compounding/day-count, date
alignment, weekend/TARGET2 carry, spread application, pre-launch boundary and
independent failure behavior through the public comparison path. They are **not**
added here because the convention they would pin is not yet verified; writing them
now would encode a guess.

## Admission decision and evidence still required

**Do not enable `estr-plus-120bp`.** The definition stays `target_only`, with no
source identifier, no admitted observation and no assignment. The exact class and
spread are recorded, but the objective's arithmetic and history are not qualified.
A dependent fund-relative comparison must remain unavailable with a precise
limitation; the fund's own facts remain available. The target's current catalog
role string is not evidence that it is an investable universe; any catalog-role
correction is a separate catalog change for the coordinator gate.

**Do not enable the objective by treating plain €STR as its construction.** The
plain €STR source is qualified separately and may be gated as its own catalog
target later. It is not the objective and does not supply the missing 120 bp
accrual convention.

To reconsider admission, obtain one of the following, with dated primary evidence:

1. A DNCA or authorized index-provider document that identifies the exact
   composite series and states the spread accrual, day count, compounding,
   weekend/holiday treatment and observation/publication-date convention.
2. A dated document establishing the effective date of the change from the
   EONIA Capitalisé +250BP predecessor and, if a predecessor is to be used, its
   exact construction and explicit approval for the splice.
3. An explicit coordinator/user decision to define the objective as an
   application-side synthetic series built from the ECB compounded €STR index plus
   120 bp under a stated formula, with that formula recorded as application
   provenance rather than presented as DNCA's published methodology.

Until one of those exists, the objective stays target-only and the dependent
window stays honestly blocked. This note does not close ticket #125 and does not
claim that any production reference, catalog assignment or comparison was enabled.

## Primary-source register

All sources were retrieved on **20 October 2026** unless stated otherwise. DNCA
documents and ECB PDFs were downloaded directly; no login or paywall was used.
Provider pages are mutable, so the dates below are the documents' own dates.

1. **D1 — PRIIPs KID, DNCA Invest ALPHA BONDS, B (EUR), ISIN LU1694789535**,
   production date **16/04/2026**, 3 pages. Read directly. p. 1: exact class,
   manufacturer, the €STR + 1.20% objective and the same performance-fee hurdle.
   <https://www.dnca-investments.com/funds/dnca-invest-alpha-bonds/parts/b-lu1694789535/KIID-b-lu1694789535/download_doc_share?country_code=INT&locale=en>
2. **D2 — DNCA Invest prospectus**, dated **20 March 2026**, read directly.
   Appendix 11 "ALPHA BONDS", printed pp. 318–322: class table with B (EUR)
   €STR + 1.20% and "Ticker of the composite: ESTRON Index" (p. 321); performance
   fee description and "multiplied by the daily return of the Index" (pp. 319–320).
   <https://www.dnca-investments.com/documents/prs_pt.pdf>
3. **D3 — DNCA Invest annual report, including audited financial statements as at
   December 31, 2025**, PDF modified **22 April 2026**, 573 pages. Read directly.
   p. 16 (Directors' Report): B EUR benchmark €STER +1.20%, performance +3.43%;
   A EUR €STER +1.40%, performance +3.63%; other class spreads. p. 247:
   performance-fee class table with B shares EUR €STER + 1.20% and "multiplied by
   the Index".
   <https://www.dnca-investments.com/funds/dnca-invest-alpha-bonds/parts/a-lu1694789451/AR-a-lu1694789451/download_doc_share?country_code=INT&locale=en>
4. **D4 — DNCA Invest Alpha Bonds monthly management report, B Share-EUR**, data
   as of **30 September 2026**, 8 pages. Read directly. p. 1: reference index
   €STR + 1.20%, 2024 +4.96%, 2025 +3.43%, 2018 +2.20%, 2019 +2.16%; p. 5:
   inception 14/12/2017 and reference index €STR + 1.20%.
   <https://www.dnca-investments.com/funds/dnca-invest-alpha-bonds/parts/b-lu1694789535/MR-b-lu1694789535/download_doc_share?country_code=INT&locale=en>
5. **D5 — DNCA Invest Alpha Bonds Management Report December 2019, Share B**,
   ISIN LU1694789535, data as of **31/12/2019**, PDF created **13 January 2020**.
   Read directly. p. 1: objective EONIA + 2.5%, footnote "EONIA Capitalisé
   +250BP", 2018 +2.20%, 2019 +2.16%.
   <https://swissfunddata.ch/sfdpub/docs/fsm-9435_13_02-20191231-en.pdf>
6. **D6 — DNCA Invest Alpha Bonds monthly management report, B Share-EUR**, data
   as of **31 December 2024**, 7 pages. Read directly. p. 1: reference index
   €STR + 1.20%, 2024 +4.96%, 2023 +5.14%, 2022 +2.10%, 2021 +2.06%,
   2020 +2.09%, 2019 +2.16%, 2018 +2.20%.
   <https://swissfunddata.ch/sfdpub/docs/fsm-9435_13_02-20241231-en.pdf>
7. **D7 — DNCA Invest Alpha Bonds monthly management report, A Share-EUR**, data
   as of **30 September 2026**. Read directly. p. 1: reference index
   €STR + 1.40%, 2024 +5.16%, 2025 +3.63%; used only to show the class-specific
   spread difference.
   <https://www.dnca-investments.com/funds/dnca-invest-alpha-bonds/parts/a-lu1694789451/MR-a-lu1694789451/download_doc_share?country_code=INT&locale=en>
8. **D8 — DNCA Invest One monthly management report, B Share**, data as of
   **31 October 2025**. Read directly. p. 2 footnote identifies the plain
   reference as **"ESTR Compounded Index"**. This corroborates that DNCA elsewhere
   names the ECB compounded index for plain €STR; it is not relied on for the
   Alpha Bonds +1.20% composite.
   <https://www.swissfunddata.ch/sfdpub/docs/fsm-9435_06_01-20251031-en.pdf>
9. **E1 — ECB, "Overview of the euro short-term rate (€STR)"**, live page read
   **20 October 2026**. States the rate is published at 08:00 CET on each TARGET2
   business day, first published 2 October 2019 reflecting trading on 1 October
   2019, the revision rule, no charge/no licence, the compounded index at 09:15,
   ISIN EU000A2QQF08, final/non-revised values, and the pre-€STR disclaimer that
   it is "for information purposes only" and "not intended for use as a benchmark
   or reference rate".
   <https://www.ecb.europa.eu/stats/financial_markets_and_interest_rates/euro_short-term_rate/html/eurostr_overview.en.html>
10. **E2 — ECB, "Compounded €STR average rates and index: Calculation and
    publication rules"**, PDF dated **17 March 2021**, 5 pages. Read directly.
    Section 2.2: index starts at 100 on 1 October 2019; formula 2 compounds each
    published rate with `n_t` days; 360-day year; 1 day Tuesday–Friday and 3 days
    on Monday under normal TARGET2 conditions; section 3: publication every
    TARGET2 business day at 09:15 CET, final and not revised.
    <https://www.ecb.europa.eu/stats/euro-short-term-rates/interest_rate_benchmarks/WG_euro_risk-free_rates/shared/pdf/ecb.Compounded_euro_short-term_rate_calculation_rules.en.pdf>
11. **E3 — ECB Data Portal, series `EST.B.EU000A2QQF08.CI`**, "Compounded Euro
    Short-Term Rate Index (1 Oct 2019 = 100)", daily business-week frequency.
    A CSV sample was downloaded through the Data API on **20 October 2026** and
    covers **2019-10-01 through 2026-10-20**; the Data Portal HTML page returned
    HTTP 503 during one check, while the CSV API and the ECB PDFs were read
    directly. This is the plain €STR series, not the DNCA composite.
    <https://data.ecb.europa.eu/data/datasets/EST/EST.B.EU000A2QQF08.CI>
    and
    <https://data-api.ecb.europa.eu/service/data/EST/B.EU000A2QQF08.CI?format=csvdata&startPeriod=2019-10-01>
12. **B1 — Bloomberg, "Bloomberg Overnight Cash Indices"**, August 2026, read
    directly. Table 2 (pp. 3–4) lists the Euro Short-Term Rate (ESTR) with
    **360** days per year, cash-rate publication lag **1**, ticker **ESTRON
    Index**, and **TARGET2 business days** as pricing days. This is the cash-rate
    definition, not DNCA's composite. It is cited to prevent `ESTRON Index` from
    being treated as a documented +1.20% series.
    <https://assets.bbhub.io/professional/sites/27/Bloomberg-Overnight-Cash-Indices.pdf>
13. **B2 — Bloomberg, "Bloomberg Cash Deposit Index Methodology"**, July 2025,
    read directly. pp. 4–5: optional spread adjustment; the spread is added to
    the cash rate before the daily `Act/DPY` accrual, with `DPY` the days per
    year. This is the only retrieved primary description of an `ESTRON`-based
    cash-plus construction; it is a Bloomberg methodology, not evidence that
    DNCA uses it. Actual Bloomberg index values were not accessed and no
    subscription or entitlement was obtained.
    <https://assets.bbhub.io/professional/sites/27/Bloomberg-Cash-Deposit-Index-Methodology.pdf>

No source observation was admitted, no production mapping was enabled, and no
network call was made from the test suite. The repository state remains
`estr-plus-120bp` and `estr` as target-only catalog definitions with unverified
production mappings; the accepted primary, secondary, Strategy and exposure
comparison behavior is unchanged.
