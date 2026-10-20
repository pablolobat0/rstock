# Carmignac Portfolio Credit — credit composite reference qualification

**Evidence checked:** 20 October 2026. **Status: conventions verified; composite not qualified as a usable comparison component.**

The approved target definition remains `75% ICE BofA Euro Corporate / 25% ICE BofA Euro High Yield`
(runtime definition id `ice-euro-corporate-75-high-yield-25`). This investigation *does* settle
several conventions that were previously open: the exact class, the component index identities and
provider codes, the total-return treatment, the EUR treatment, and the **externally specified reset
convention**. It does **not** establish permitted access to, or usable coverage of, the two component
series, so no component observation is admitted, no production mapping is enabled, and the definition
stays `target_only`. The existing comparison gate therefore keeps the fund-relative facts
*unavailable* with a precise reason, while the fund's own NAV, holdings and inventory facts remain
available and unaffected.

## Outcome

**The reset convention is verified and it is quarterly, not the saved custom-blend monthly rule.**
Carmignac's own Key Information Document for the exact class states: *"The reference indicator is 75%
ICE BofA Euro Corporate index and 25% ICE BofA Euro High Yield index. The sub-fund's reference
indicator is rebalanced quarterly."* (KID, date of production 16/04/2026). The 30/09/2026 monthly
factsheet repeats it as *"Rebalancing Frequency: Quarterly"*, and Carmignac's October 2026 marketing
presentation states *"Benchmark index: 75% ICE BofAML Euro Corporate (ER00) (coupons reinvested), 25%
ICE BofAML Lynch Euro High Yield (coupons reinvested). Rebalanced quarterly."* The two components are
therefore ICE BofA **ER00** and **HE00**, on a total-return (coupons-reinvested) basis, in EUR, with
quarterly weight resets.

**The access and coverage gates remain open, and they are asymmetric.** ICE's own Index Platform
publishes ER00 and HE00 but its index pages, rules and factsheets sit behind ICE SSO authentication
(`https://indices.ice.com/api/index/ER00` returned the "ICE SSO Client" sign-in page on 20 October
2026). The only public redistribution of ICE index *levels* found is FRED's "ICE BofA Indices" release
(sourced to Ice Data Indices, LLC), and that release carries **only** the HE00 series — its OAS,
effective yield, semi-annual yield-to-worst and total-return index value — with **no ER00 series at
all**. Its HE00 total-return level is a rolling window: *"Starting in April 2026, this series will only
include 3 years of observations. For more data, go to the source."* The 75% component therefore has no
public daily level series, and the 25% component has a public daily series that neither covers the
class history (first NAV 31/07/2017) nor permits the application's use: the same series notes state
*"Reproduction of this data in any form is prohibited except with the prior written permission of ICE
Data Indices"*, that end-of-day index values and returns are *"provided for your internal use only"*
and may not be *"publish[ed], distribute[d] or otherwise furnish[ed] to any third-party without prior
written approval"*, and that the indices may not be used as a reference index for financial products
without prior written approval of ICE Data. FRED tags the series *"Copyrighted: Pre-Approval
Required"*.

This is a qualification blocker, not a claim that ICE does not offer the data. It is the same
situation recorded for the equity and cat-bond reference qualifications: the target definition stays
selected and honest, and dependent results stay unavailable until the identifiers, entitlement,
coverage and payload are evidenced.

## Findings by gate

| Gate | Evidence and conclusion | Status |
| --- | --- | --- |
| Exact class, vehicle and share-class identity | The KID (production date 16/04/2026) names product *Carmignac Portfolio Credit*, manufacturer Carmignac Gestion Luxembourg, **share class A EUR Acc, ISIN LU1623762843**, an accumulation share; the sub-fund was launched on 31/07/2017 as a sub-fund of the Carmignac Portfolio SICAV (CSSF registered 2530). The 30/09/2026 factsheet records *"Date of 1st NAV 31/07/2017"*, Bloomberg *CARUCAE LX*, base currency EUR, dividend policy Accumulation, fund currency EUR. The same factsheet lists the sibling classes with their own ISINs and first-NAV dates — **FW EUR Acc (LU1623763148)**, **F EUR Acc (LU1932489690)**, **A USD Acc Hdg (LU1623763064)**, **A CHF Acc Hdg (LU2020612490)**, **F USD Acc Hdg (LU2020612904)** — so the A EUR Acc history may not be borrowed from any of them. | **Verified** |
| Component identities and provider series codes | Carmignac's October 2026 presentation names the components with ICE's own codes: *"75% ICE BofAML Euro Corporate (ER00) (coupons reinvested), 25% ICE BofAML Lynch Euro High Yield (coupons reinvested)"*, and labels its charts *"Euro Corporate IG index (ER00)"* and *"Euro HY index (HE00)"*. ICE's own Index Platform finder, as rendered on 20 October 2026, lists *"Euro Corporate ER00"* and *"Euro High Yield HE00"* with daily levels; ICE's published Bond Index Almanac (2019) uses *"Euro Corporate Index (ER00)"*; ICE's 17 June 2020 press release names *"ICE BofA Euro Corporate Index"* and *"ICE BofA Euro High Yield Index"* among the indices moved to real-time publication. Third-party corroboration of the same code↔name mapping: ECB BMCG Item 2B (22 June 2022) and BlackRock's BGF Euro Corporate Bond Fund factsheet (September 2026), which cites *"ICE BofA Euro Corporate Index (ER00) (EUR)"*. | **Verified** |
| Return / total-return variant | Carmignac specifies *"coupons reinvested"* for both components, i.e. the total-return variant of each index, not a price index and not a gross-return variant. ICE publishes the variant as a distinct series: FRED carries *"ICE BofA Euro High Yield Index **Total Return Index Value**"* separately from the OAS, effective-yield and yield-to-worst analytics of the same HE00 index. **Caution:** ICE's platform snapshot and the total-return level are not the same value — the rendered ICE Index Platform snapshot for 19 October 2026 shows HE00 at 389.28 while the ICE-sourced total-return level for the same date is 390.42 — so the platform's generic "Index Value" must not be used as the total-return series. The exact ICE series code for each component's total-return variant (and for ER00 at all) was not obtained. | **Partially verified** — variant is total return; the exact per-component TR series codes are unresolved |
| EUR treatment and hedging | Both components are EUR-denominated Euro-market indices (ER00: EUR-denominated investment-grade corporate debt; HE00: EUR-denominated below-investment-grade corporate debt), so the composite is an unhedged EUR series and no FX conversion is involved. The A EUR Acc class is not a currency-hedged class: the hedged classes are separately identified share classes with their own ISINs (A USD Acc Hdg LU1623763064, A CHF Acc Hdg LU2020612490, F USD Acc Hdg LU2020612904, X2 CAD Acc Hdg LU2772084310) and are out of scope here. EUR denomination is not hedging: the KID states the sub-fund uses derivatives "for hedging or arbitrage purposes, or to expose the portfolio to the following risks (directly or via indices): credit (up to 30% of the net assets), interest rates, currencies, equities and dividends", and the fund's Q3 2026 letter discloses *"Our hedging position through CDS on the Xover (euro high yield) index is sized at 18.7% of the portfolio"* (data as of 30/09/2026). That is a **fund-side** deviation to disclose, not a change to the reference's convention. | **Verified** |
| Externally specified reset convention | Three independent Carmignac primary documents agree: KID 16/04/2026 *"The sub-fund's reference indicator is rebalanced quarterly"*; monthly factsheet 30/09/2026 *"Rebalancing Frequency: Quarterly"*; October 2026 presentation *"Rebalanced quarterly"*. The composite therefore **must not** inherit the saved custom-blend monthly reset rule, and it must not be flattened into a static 75/25 daily rebalance. The retrieved documents state the *frequency* but not the *anchor close* (which quarter-end close the reset takes place at); that sub-detail remains unevidenced. Separately, each component index rebalances its own constituents monthly on ICE's rule (*"The Index is rebalanced on the last calendar day of the month, based on information available up to and including the third business day before the last business day of the month"*, ICE-authored note on the HE00 series) — that is component-internal and is not the composite's weight reset. | **Verified** (frequency) / **Unresolved** (exact reset anchor close) |
| Component source identifiers and permitted access | ICE's Index Platform is the primary publisher, and its per-index pages, rules and factsheets are behind ICE SSO authentication (verified 20 October 2026). ICE's 2021 and 2026 fixed-income brochures describe the family but not ER00/HE00 individually. The public FRED redistribution covers HE00 only, under terms that prohibit reproduction without prior written permission, restrict use to internal use only, and forbid third-party distribution and financial-product use without prior written approval of ICE Data. No entitlement, licence, sample payload or approved feed contract for this application was produced. | **Unresolved** — this is the blocking gate |
| Observation cadence | ICE publishes both components in real time and end of day: ICE moved *"ICE BofA Euro Corporate Index"* and *"ICE BofA Euro High Yield Index"* to real-time publication (press release, 17 June 2020), and the ICE-sourced HE00 total-return series is published *"Daily, Close"*, with a 20 October 2026 publication carrying the 19 October 2026 close. The *usable* cadence for an admitted ER00 series was not established, because no ER00 payload was obtained. | **Verified for cadence, not for the ER00 payload** |
| Historical boundaries and coverage | The class history starts 31/07/2017. The only public component series carries a rolling 3-year window (*"Starting in April 2026, this series will only include 3 years of observations"*), which cannot cover the class history even if it were permitted to use. ICE's own index history depth for ER00/HE00 was not obtained from an ICE primary document, and no launch/base date for either total-return series was established. | **Unresolved** |
| Adjustment provenance | ICE's own construction notes for HE00 document capitalization weighting, accrued interest on next-day settlement, cash flows retained until month-end and removed at rebalancing, and exclusion of defaulted, warrant-bearing and euro legacy currency securities. An ICE-authored equivalent description for ER00 was not retrieved (the description circulating in third-party material is a quotation of ICE's text, not the ICE document). For the application the operative rule is that an index level is *already* a total-return series: no dividend or adjustment factor may be re-applied to it, and the price-return level may not be substituted for the total-return level. | **Partially verified** |
| Comparability limitation (a limitation, not a mapping blocker) | The KID is explicit: *"the sub-fund's investment universe is mostly independent from the indicator… the sub-fund's holdings and the weightings may substantially deviate from the composition of the indicator. There is no limit set on the level of such deviation."* The mandate is global including emerging countries, may hold up to 50% below investment grade, up to 10% equities, up to 20% CoCos, securitisation, and long/short and relative-value strategies. The composite is therefore the fund's **official performance-fee and outperformance yardstick**, not a holdings replica: the reference is Euro-only IG/HY while the fund is global credit with a 50% below-IG allowance. Carmignac's own product sheet describes the portfolio construction as *"Non-benchmarked: Portfolio construction is a result of Fund manager views and market analysis with no bias to any benchmark."* | **Verified** — must be disclosed with the comparison |

## What a named external composite must preserve

A future implementation of this reference (behind the shared Benchmark Module, not as a user-nested
blend) must carry the verified convention explicitly and must not quietly re-derive it:

- **Identity.** One named external composite: 75% ICE BofA Euro Corporate (ER00) + 25% ICE BofA
  Euro High Yield (HE00), EUR, total return (coupons reinvested), quarterly reset. It is a
  *definition with components and an external schedule*, not a saved user blend.
- **Reset.** Weights are reset to 75/25 on the externally specified quarterly schedule, not the
  saved custom-blend monthly rule, and not a flat 75/25 daily rebalance. The reset anchor close is
  still unevidenced; until it is, the schedule is *quarterly, anchor unknown* and dependent results
  must be unavailable rather than guessed.
- **Component wealth.** Component returns are evaluated with beginning-of-interval weights and
  compounded into component wealth; the composite is the weighted sum of component wealth at the
  reset, drifting with component wealth between resets. It is not a weighted average of log returns.
- **No flattening.** The composite keeps its two components, their weights, their return basis and
  their reset rule visible in human and JSON output. It does not expose arbitrary user nesting and
  does not erase the external schedule by flattening it into a single synthetic series.
- **Unavailability is honest.** A missing required component, an unknown reset convention or a
  source outage makes the dependent comparison *unavailable* with a precise reason. It never
  renormalizes the remaining weights, substitutes the other component, switches to a price-return
  level, or silently shortens the comparison window.
- **Provenance.** The rendered comparison carries the components, weights, currency, return basis,
  hedge policy, cadence, source identifiers, reset rule and document provenance, so the meaning
  survives outside the database.

## Admission decision and evidence still required

**Do not enable the composite as a usable component.** The conventions are now verified; the *data
gate* is not. The definition stays `target_only` and the fund comparison stays unavailable with the
existing precise reason rather than a false success.

To reconsider admission, obtain:

1. Dated, written permission (or a distribution agreement) from ICE Data Indices covering the
   retrieval, caching, comparison and display of the ER00 and HE00 total-return series by this
   application — the FRED route's own terms are not sufficient, and an HTTP 200 from a public
   endpoint is not a licence.
2. The exact ICE series identifiers for the **total-return** variant of ER00 and of HE00, together
   with the price-return counterparts so the two are never interchanged, and confirmation of the
   EUR (unhedged) variant rather than a hedged-to-USD variant.
3. A documented feed contract or sample payload showing observation dates, values, cadence,
   revision/correction policy and the exact reset-relevant close for each component.
4. Coverage for the full comparison window with the launch versus back-test boundary stated for
   both components, and reconciliation proving the return basis, currency treatment and that no
   adjustment is applied twice.
5. The exact quarterly reset anchor convention from a dated primary source, or an explicit decision
   that the comparison cannot be computed without it.

Until (1) to (4) exist, no ER00/HE00 observation may enter the cache, and the fund's benchmark-relative
facts must remain unavailable while the fund's own facts stay available.

## Behavioural contract for the eventual implementation (offline, hand-calculated)

When admission is unblocked, the offline suite must pin the verified schedule rather than a guessed
one, through the public fund-comparison flow with fake source Adapters behind `MarketData`:

- a fixture with a known quarterly reset crossing proves the 75/25 weights are restored at the
  external quarterly anchor and drift with component wealth in between — and that this differs
  from the monthly custom-blend schedule on the same inputs;
- a fixture with a missing component proves the comparison is *unavailable* with a reason, and that
  the surviving weight is **not** renormalized to 100% and the window is **not** shortened;
- a fixture with a source outage on one component proves the same unavailability, with the cached
  provenance of the surviving component retained and no substitution;
- a fixture with a price-return level in place of a total-return level proves the application
  rejects the mismatched adjustment basis instead of labelling it total return;
- exact dates — class first NAV 31/07/2017, reset anchors, and the comparison start/end — are
  asserted, not approximated.

## Primary-source register

Carmignac's own documents are cited, not reproduced: the product sheet states it "may not be
reproduced, in whole or in part, without prior authorisation from the management company". Primary
documents downloaded to a scratch directory outside the repository on 20 October 2026; the dated
excerpts below are the minimum needed to identify each convention.

1. **Carmignac Gestion Luxembourg**, *Key Information Document — Carmignac Portfolio Credit A EUR
   Acc (LU1623762843)*, **date of production 16/04/2026**; accessed 20 October 2026.
   <https://www.carmignac.com/assets/sequantis/kideu_LU1623762843_en_LU/KIDEU_Carmignac-Portfolio-Credit-A-EUR-Acc_LU1623762843_INTL_en.pdf>
   — class, ISIN, launch date, the 75/25 reference indicator, *"rebalanced quarterly"*, the
   actively-managed/unconstrained and global-universe statements, the 50% below-investment-grade
   allowance, and the derivative/hedging permissions.
2. **Carmignac Gestion**, *Monthly Factsheet — Carmignac Portfolio Credit A EUR Acc*, **data as of
   30/09/2026**; accessed 20 October 2026.
   <https://www.carmignac.com/assets/yoda/FLFPRO_CCR_LU1623762843_INT_en/Monthly-Factsheet-PRO_Carmignac-Portfolio-Credit-A-EUR-Acc_LU1623762843_INTL_en.pdf>
   — *"75% ICE BofA Euro Corporate Index + 25% ICE BofA Euro High Yield index"*, *"Rebalancing
   Frequency: Quarterly"*, first NAV 31/07/2017, EUR, Accumulation, sibling-class list.
3. **Carmignac Gestion**, *Product Sheet — Carmignac Portfolio Credit* (**Carmignac at 30/09/2026**);
   accessed 20 October 2026.
   <https://www.carmignac.com/assets/yoda/PSPRO_CCR_FI_en/Product-Sheet-Pro_Carmignac-Portfolio-Credit_FI_en.pdf>
   — reference-indicator footnote, class table (A EUR Acc LU1623762843, FW EUR Acc LU1623763148,
   F EUR Acc LU1932489690), 1.2% management fee, 20% performance fee against the reference indicator
   with a 5-year clawback, and the "non-benchmarked" portfolio-construction statement.
4. **Carmignac Gestion**, *Carmignac Portfolio Credit — marketing presentation*, **October 2026**
   (Carmignac data 30/09/2026); accessed 20 October 2026.
   <https://www.carmignac.com/uploads/pdf/0001/31/ff64964b22b4b25064b595efeaec61a14f24be52.pdf>
   — the only retrieved document that states the component codes and return treatment: *"75% ICE
   BofAML Euro Corporate (ER00) (coupons reinvested), 25% ICE BofAML Lynch Euro High Yield (coupons
   reinvested). Rebalanced quarterly."*
5. **Carmignac Gestion**, *Carmignac P. Credit: Letter from the Fund Managers — Q3 2026*, data as of
   30/09/2026 (the parent research record dates the visible publication 13 October 2026); accessed
   20 October 2026.
   <https://www.carmignac.com/en-fr/articles/carmignac-p-credit-letter-from-the-fund-managers-q3-2026-4127-14034>
   — *"Reference Indicator: 75% ICE BofA Euro Corporate Index + 25% ICE BofA Euro High Yield index.
   Rebalancing Frequency: Quarterly"*, *"Our hedging position through CDS on the Xover (euro high
   yield) index is sized at 18.7% of the portfolio"*, and the quarterly performance table.
   **Retrieval note:** the direct page and PDF fetch returned the site's
   bot-mitigation redirect on 20 October 2026; this item is indexed primary text, not a document
   downloaded and inspected. The Q4 2025 letter
   (<https://www.carmignac.com/en/articles/carmignac-p-credit-letter-from-the-fund-managers-q4-2025-3599-12557?_format=pdf>)
   is retained in the same way; it shows the legacy *BofA Merrill Lynch* naming of the same two
   indices, which is a branding continuity, not a different benchmark.
6. **Intercontinental Exchange (ICE Data Services)**, *ICE Data Services Launches Real-Time
   Publication of Key Fixed Income Indices*, **17 June 2020**; accessed 20 October 2026.
   <https://ir.theice.com/press/news-details/2020/ICE-Data-Services-Launches-Real-Time-Publication-of-Key-Fixed-Income-Indices/default.aspx>
   (PDF copy at <https://s2.q4cdn.com/154085107/files/doc_news/ICE-Data-Services-Launches-Real-Time-Publication-of-Key-Fixed-Income-Indices-2020.pdf>)
   — names *"ICE BofA Euro Corporate Index"* and *"ICE BofA Euro High Yield Index"* as indices
   administered by ICE Data Indices, LLC and published in real time, evidencing a daily publication
   regime for both components.
7. **ICE**, *Index Platform*, <https://indices.ice.com/>; accessed 20 October 2026. The index finder
   lists *"Euro Corporate ER00"* and *"Euro High Yield HE00"* with daily levels. The per-index pages,
   rules and factsheets are authentication-gated: `https://indices.ice.com/api/index/ER00` returned the
   "ICE SSO Client" sign-in page. A rendered snapshot dated 19 October 2026 shows HE00 389.28 and
   ER00 314.05, which differ from the ICE-sourced total-return level for HE00 (390.42, item 8), so the
   platform's headline level is not established as the total-return series.
8. **Ice Data Indices, LLC** (release *"ICE BofA Indices"*, redistributed by the Federal Reserve Bank
   of St. Louis), *ICE BofA Euro High Yield Index Total Return Index Value*,
   **series BAMLHE00EHYITRIV**; accessed 20 October 2026.
   <https://fred.stlouisfed.org/series/BAMLHE00EHYITRIV> — frequency *"Daily, Close"*; 2026-10-19:
   390.42; the rolling three-year observation window; ICE's own index description and construction
   notes (capitalization weighting, next-day settlement accrued interest, month-end cash retention
   and rebalancing); and the ICE "Top Level Data" licence terms quoted in the Outcome section.
9. **Ice Data Indices, LLC** (release *"ICE BofA Indices"*), release index, accessed 20 October 2026.
   <https://fred.stlouisfed.org/release?rid=209> — the release's only Euro series are the four HE00
   series (option-adjusted spread, effective yield, total-return index value, semi-annual yield to
   worst). **No ICE BofA Euro Corporate (ER00) series is present**, so the 75% component has no public
   daily level series.
10. **ICE**, *Bond Index Almanac*, published 18 April 2019; accessed 20 October 2026.
    <https://www.ice.com/publicdocs/Bond_Index_Almanac_Report.pdf> — ICE's own naming of
    *"Euro Corporate Index (ER00)"* and its ER00 sector tables. Used for identity corroboration only;
    the document states it may not be reproduced without express permission.
11. **ICE**, *ICE Fixed Income Indices*, 6 March 2026, and *ICE Fixed Income Index Family*,
    30 September 2021; accessed 20 October 2026.
    <https://www.ice.com/publicdocs/events/ICE_Fixed_Income_Indices.pdf> and
    <https://www.ice.com/publicdocs/ICE_Fixed_Income_Index_Family.pdf> — the offering context and
    the "available to all ICE Index Platform subscribers" access model. Neither brochure describes
    ER00 or HE00 individually.

## Repository state this evidence supports (not applied by this change)

The runtime state stays honest as-is: `benchmark_references.id = 'ice-euro-corporate-75-high-yield-25'`
remains `source = 'unverified'`, `verification_state = 'target_only'`, with the catalogue strings
*"Total-return variants and reset schedule unverified"*, hedge policy *"Unknown"* and history boundary
*"History boundaries unknown"*. This evidence justifies correcting those three descriptive strings
to the now-verified conventions (total return with coupons reinvested; quarterly external reset;
EUR unhedged) and to recording the partial boundaries, but it does **not** justify changing
`verification_state` to `verified`, adding a source identifier, or admitting observations, because the
access and coverage gates above are still open. Any such change is an implementation decision for the
coordinator's gate, and this change records evidence only.

No source Adapter, production mapping, schema change, CLI surface, cache entry or real portfolio
record was touched by this investigation. No network access occurred inside the test suite.
