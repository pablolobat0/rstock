# Duration-sensitive bond reference qualification

**Evidence checked:** 19–20 October 2026. **Status: neither mapping is qualified for usable comparison.**

The approved targets remain *Bloomberg Euro Aggregate 1–3 Years* for the Neuberger class
IE00BFZMJT78 and *Bloomberg Global Aggregate 1–5 Year, total return, EUR-hedged* for the Ábaco
bond sleeve. This note records what dated primary evidence establishes, what it does not, and
why no mapping is admitted. It does not approve a substitute, a proxy, another class, another
variant, a shorter period, or a converted series in place of a hedged one.

## Outcome

**No mapping passes the source-qualification gate.** For both references the exact provider series
identifier, the permitted access for this application, and a validated daily observation payload
with usable coverage remain unresolved. Nothing is enabled, no observation is admitted, and no
production mapping is changed by this work. The definitions stay target-only, so a comparison
against them stays unavailable with a precise limitation instead of a substituted series.

The inability to prove a complete input is not a claim that Bloomberg does not publish these
indices. It is a qualification blocker: obtain the exact series identifiers and dated
entitlement from Bloomberg or an authorized distributor, then validate the returned series,
its conventions, and its boundaries before enabling it.

## Neuberger Short Duration Euro Bond Fund — exact class IE00BFZMJT78

| Gate | Evidence and conclusion |
| --- | --- |
| Exact class identity | **Verified.** The exact class is *EUR I Acc, ISIN IE00BFZMJT78*, share-class inception **2018-10-31**, fund inception 2018-10-31, base currency EUR, Ireland-domiciled UCITS, daily valuation. The fund is a sub-fund of **Neuberger Berman Investment Funds PLC**, authorised by the Central Bank of Ireland. The exact-class factsheet is issued by **Neuberger Berman Asset Management Ireland Limited** in the EEA, regulated by the Central Bank of Ireland, and by **Neuberger Berman Europe Limited** for the United Kingdom and outside the EEA; the latter is the issuer of that document there, not the fund's management company. [N1, N2, N3] |
| Comparator of that exact class | **Verified.** The fund's own factsheet names the benchmark *"Bloomberg Euro Aggregate 1-3 Years (Total Return, EUR)"* for the exact EUR I Acc class; the same statement appears in an independent factsheet copy for the same ISIN. The fund states it is actively managed and that the benchmark is used for comparison purposes only. [N1, N2, N3] |
| Return basis and currency of the benchmark | **Verified for the benchmark statement.** The series is a *total return* series published in *EUR*. The Euro Aggregate family measures the investment-grade, **euro-denominated** market, so the series is published in its own index currency: it is neither a currency-converted foreign-currency index nor a hedged variant. [N1, N2, B1] |
| Currency-hedge policy of the exact class | **Unverified.** The inspected exact-class documents state the class identity, its EUR denomination, and a euro-denominated investment universe, but none of them states whether the class hedges currency exposure. A EUR denomination and a predominantly euro-denominated portfolio are not evidence of a hedge policy, so no class-level hedge statement is carried into the reference definition and the class policy stays unresolved. [N1, N2, N3] |
| Index family identity | **Partially confirmed.** The parent index is the *Bloomberg Euro Aggregate Index*, **Index ID 2000**, tickers **I02000EU** / **LBEATREU:IND** (*Total Return EUR Unhedged*), launched **1998-06-01**, covering treasury, government-related, corporate, and securitized investment-grade euro-denominated fixed-rate bonds with at least one year to final maturity. Bloomberg publishes maturity-bucketed members of this family, for example *Euro-Aggregate: Treasury 1-3 Year* (**I02119EU** / LET1TREU:IND) and *Euro-Aggregate: Corporate 1-3 Year* (**I02134US:IND**). [B1, B2, B3] |
| Exact 1–3 year series identifier | **Unresolved.** The all-sector *Euro Aggregate 1–3 Year* index identifier and ticker were not confirmed from a primary provider page in this investigation. The 1–3 year bucket is evidenced as a *benchmark statement* of the exact class, not as a confirmed series key. Do not infer one from the parent index or from a sibling bucket. |
| Observation cadence, usable coverage, launch versus back-test boundary | **Unresolved.** No dated primary document or accessible payload establishes the daily publication schedule, the observation history, the correction or revision policy, or the launch date of the 1–3 year series itself. The parent index launch of 1998-06-01 is not the series history. |
| Permitted access | **Unresolved.** Bloomberg's fixed-income index methodology states that index subscribers may receive index data in files, that the files may carry index-level and constituent-level returns and characteristics, that index data is also available via authorized redistributors, and that Bloomberg requires index data licenses for services and products linked to the Indices. [B4, p. 63] No license, contract, or written permission permitting this application to retrieve, cache, compare, and display the series was produced, and this investigation did not obtain a dated primary statement of a delivery channel this application may use. Public index pages, vendor list pages, and paid third-party chart pages are discovery, not a permitted feed. |
| Class history boundary | **Verified.** The exact class starts on 2018-10-31, and the fund inception of the same date does not extend it. The EUR I Dist class of the same fund also starts on 2018-10-31, so it is not an example of a class with a different history; the classes of the same fund that start later are the USD I Acc (15-09-2021), GBP I Dist (28-11-2025), and JPY I Acc (19-12-2025) classes. No earlier class history is authorized, and the benchmark statement of the exact class is not extended to another class. [N2, N3] |

## Ábaco Renta Fija Mixta Global C — exact class ES0140072028

| Gate | Evidence and conclusion |
| --- | --- |
| Exact class identity | **Verified.** The exact class is *C*, **ISIN ES0140072028**, class registered **2017-12-01**, denomination currency EUR, accumulation class, in *ABACO Renta Fija Mixta Global, FI* (formerly ABACO GLOBAL, FI), fund constituted 2012-04-23, CNMV fund registration 4474, manager ABACO CAPITAL SGIIC. [A1] |
| Official comparator of that class | **Verified as absent.** The prospectus states: *"No existe un indice de referencia dado que se realiza una gestión activa y flexible."* The fund has **no reference index**. The 80% Bloomberg Global Aggregate 1–5 Year / 20% MSCI ACWI Small Cap Value reference is therefore an **independently chosen stable comparison target**, not a fund comparator, and it is labelled as such. [A1, P1] |
| Mandate facts that bound the comparison | **Verified.** The prospectus allows up to 30% equity, states that the currency risk may range between **0% and 50% of total exposure**, that there is no predetermined fixed-income duration (it may even be negative at times), and that any credit quality — including unrated, and even a wholly low-quality portfolio — may be held. A EUR-hedged short-maturity bond sleeve is therefore a deliberate, disclosed comparator choice and not a replica of the fund's risk. [A1] |
| Selected sleeve identity | **Partially confirmed.** *Bloomberg Global Aggregate 1-5 Year* is evidenced as a published index: the iShares Global Aggregate 1-5 Year Bond Index Fund factsheet of September 2026 names the benchmark *"BBG Global Aggregate 1-5 Year Index (USD)"* with index ticker **I31413US**, covering multi-currency investment-grade securities with 1–5 year maturities. The parent *Bloomberg Global Aggregate* is published as **LEGATRUU:IND** (Total Return Unhedged USD, future ticker I00038US). [I1, I2, B5] |
| EUR hedging of that sleeve | **Not established for the selected series, and explicitly distinguished from conversion.** Bloomberg's fixed-income methodology states that returns on multi-currency indices are calculated on both an unhedged and a currency-hedged basis in several reporting currencies, and a *Bloomberg Global Aggregate EUR Hedged* variant is listed on third-party index pages. The iShares fund's EUR-hedged share classes hedge **inside the share class** with FX forwards while still tracking the **USD** index; a share-class hedge is therefore not the EUR-hedged index variant, and converting the unhedged USD index into EUR is a conversion, not a hedge. The exact *1–5 Year EUR-hedged* series identifier was not confirmed from a primary provider page. [I1, I3, B4 p. 12, B6, T1] |
| Observation cadence, usable coverage, historical boundaries | **Unresolved.** No accessible payload or dated primary document establishes the daily schedule, the observation history, the correction policy, or the launch-versus-back-test boundary of the exact EUR-hedged 1–5 year series. |
| Permitted access | **Unresolved.** Same position as the Neuberger reference: index data is delivered to subscribers in files and via authorized redistributors under an index data license, and no license, contract, or written permission permitting this application to retrieve, cache, compare, and display the series was produced. [B4, p. 63] |

## Conversion is not hedging

The two references are different kinds of comparison and must not be merged:

* The Neuberger benchmark is a **euro-denominated** total-return series. Its EUR figures are the
  index's own currency, so no conversion and no hedge is involved. This is a statement about the
  index series only; the exact class's own currency-hedge policy remains unverified above.
* The Ábaco sleeve requires the **EUR-hedged** variant of a global, multi-currency index. An
  unhedged USD series expressed in EUR — whether by conversion at the application level or by a
  fund's hedged share class — does not reproduce the currency-hedged return of the index and is
  not admitted as a substitute. The iShares evidence above is the concrete case: the same
  benchmark line, hedged at the share-class level, is a different investment from the EUR-hedged
  index series.

Bloomberg's own methodology states the limit of what may be inferred here: returns on
multi-currency indices are calculated on both an unhedged and a currency-hedged basis in several
reporting currencies, and the euro-denominated Euro Aggregate may be offered in additional
currencies for both unhedged and hedged indices. [B4, p. 12; B1, p. 2] The currency of a series
therefore establishes neither hedging nor the absence of it: a mere currency conversion is not a
hedge, and an other-currency series may be hedged or unhedged, so the exact hedging policy of the
series used has to be verified from the provider's own series description. The Ábaco sleeve's
exact EUR-hedged series stays one of the unresolved gates above.

## Admission decision

Do not enable either reference as a usable component. `bloomberg-euro-aggregate-1-3y` and
`bloomberg-global-aggregate-1-5y-eur-hedged` stay target-only: the comparison gate must keep
reporting them as unavailable with a precise limitation, and it must not shorten the comparison
window, extend a class's history, substitute a corporate-only or unhedged index, fill internal
gaps, or present a converted series as a hedge. The Ábaco mixed reference of the following
ticket cannot use the bond sleeve while the sleeve is unqualified.

To reconsider admission, obtain for each mapping independently: (1) the provider-issued exact
series identifier and variant (total return, published currency, hedging) for the *exact* series
named above, including the maturity bucket and, for the Ábaco sleeve, the EUR-hedged variant;
(2) dated evidence of access rights permitting this application to retrieve, cache, compare, and
display it; (3) a sample or documented feed contract that identifies observation dates, values,
cadence, and the correction and revision policy; (4) the usable start and end coverage, with the
launch versus back-test boundary stated; and (5) reconciliation proving the return basis, the
currency treatment, no double adjustment, and immutable handling of completed dates.

## Primary-source register

All sources were retrieved on **19–20 October 2026**. Provider pages are mutable; the document
dates below are the ones the documents themselves carry. Where a document could not be fetched
directly, the entry says so: `nb.com` answered **HTTP 429** to direct requests for the two
Neuberger factsheets, and `bloomberg.com` answered **HTTP 403** for its index pages, so those
were read through indexed primary text; the Swissfunddata factsheet, the Ábaco prospectus, and
both Bloomberg methodology documents were downloaded and read directly.

- **N1:** Neuberger, *Short Duration Euro Bond Fund* factsheet, published **2026-06-30**
  (exact EUR I Acc class IE00BFZMJT78; benchmark "Bloomberg Euro Aggregate 1-3 Years (Total
  Return, EUR)"; class inception 31-10-2018; "Source: Neuberger Berman, BlackRock Aladdin, as
  of June 30, 2026"), <https://www.nb.com/api/marketing/file?itemid=3264D7EF472A4A6DA050242E3BE69AEF&v=2>
  — indexed primary text; the direct request returned HTTP 429.
- **N2:** Neuberger, *Short Duration Euro Bond Fund* factsheet, **September 2026** edition (data to
  30 September 2026; same exact class, same benchmark statement, rebranding note of 4 August
  2026, share-class table with the EUR I Dist, GBP I Dist, JPY I Acc, and USD I Acc classes and
  their own inception dates),
  <https://www.nb.com/api/marketing/file?itemid=772EB6E072B4498A8F9A1E2620A7BBC1&v=3>
  — indexed primary text; the direct request returned HTTP 429.
- **N3:** Swissfunddata, *Neuberger Berman Short Duration Euro Bond Fund EUR I Accumulating
  Class* factsheet dated **2025-07-31**, read directly (six pages). It carries the same
  benchmark statement for the same ISIN, the share-class table (EUR I Acc 31-10-2018 IE00BFZMJT78;
  EUR I Dist 31-10-2018 IE00BFZMJS61; USD I Acc 15-09-2021), the "actively managed, not
  constrained by its benchmark" statement, and the issuer attribution used above
  (sub-fund of Neuberger Berman Investment Funds PLC; document issued by Neuberger Berman Asset
  Management Ireland Limited in the EEA and by Neuberger Berman Europe Limited for the United
  Kingdom and outside the EEA), <https://www.swissfunddata.ch/sfdpub/docs/fsm-8350_92_05-20250731-en.pdf>.
- **A1:** ABACO CAPITAL, *Folleto — ABACO Renta Fija Mixta Global, FI*, CNMV fund registration
  **4474**, last updated **2026-06-08**, read directly (ten pages; "Última actualización del
  folleto: 08/06/2026"; fund constituted 23/04/2012, CNMV registration 11/05/2012; class C ISIN
  ES0140072028 registered 01/12/2017; "No existe un indice de referencia dado que se realiza una
  gestión activa y flexible"; "El riesgo divisa podrá oscilar entre 0%-50% de la exposición
  total"; "No está predeterminada la duración media de la cartera de renta fija, pudiendo incluso
  ser negativa puntualmente"),
  <https://www.abaco-capital.com/wp-content/Documentacion/folleto_ARFMG.PDF>.
- **B1:** Bloomberg, *Bloomberg Euro Aggregate Index* methodology, read directly (four pages; the
  document carries the date 29 September 2026). Index ID 2000; base-currency series I02000EU /
  LBEATREU:IND, Total Return EUR Unhedged; launched 1 June 1998; eligibility rules. Page 2
  (Publication Currency): "Bloomberg may offer this index in additional currencies for both
  unhedged and hedged indices", with a cross-reference to Appendix 2 of the Fixed Income Index
  Methodology for currency hedging and currency returns.
  <https://assets.bbhub.io/professional/sites/27/Euro-Aggregate-Index.pdf>.
- **B2:** Bloomberg, *Bloomberg Euro-Aggregate: Treasury 1-3 Year* index page (LET1TREU:IND, future ticker I02119EU),
  <https://www.bloomberg.com/professional/products/indices/quote/LET1TREU:IND> — indexed primary text; the direct page returned HTTP 403 to this investigation.
- **B3:** Bloomberg, *Bloomberg Euro-Aggregate: Corporate 1-3 Year* index page (I02134US:IND),
  <https://www.bloomberg.com/professional/products/indices/quote/I02134US:IND> — same retrieval note.
- **B4:** Bloomberg, *Fixed Income Index Methodology*, published **2026-01-08**, read directly
  (114 pages). Page 12 (Currency): returns on multi-currency indices are calculated on both an
  unhedged and a currency-hedged basis in several reporting currencies, and for single-currency
  indices returns are also available in currencies other than that of the index. Page 63 (Data
  Distribution and Index Licensing): index subscribers may choose to receive index data in files,
  which may carry index-level and/or constituent-level returns and characteristics; index data is
  also available via authorized redistributors; Bloomberg requires index data licenses for
  services and products linked to the Indices.
  <https://assets.bbhub.io/professional/sites/10/Bloomberg-Index-Publications-Fixed-Income-Index-Methodology.pdf>.
- **B5:** Bloomberg, *Bloomberg Global-Aggregate Total Return Index Value Unhedged USD* index page (LEGATRUU:IND, future ticker I00038US),
  <https://www.bloomberg.com/professional/products/indices/quote/legatruu:ind> — same retrieval note.
- **B6:** Bloomberg, *Bloomberg Fixed Income Indices* product page (key-index table with tickers
  such as LBEATREU; index-level and constituent data described as available on third-party
  platforms and through the Terminal),
  <https://www.bloomberg.com/professional/products/indices/fixed-income/> — same retrieval note.
- **I1:** BlackRock, *iShares Global Aggregate 1-5 Year Bond Index Fund (IE)* factsheet, **September 2026** (benchmark "BBG Global Aggregate 1-5 Year Index (USD)"; class D Hedged EUR IE00BMZ3NN11; share class launched 13-May-2021),
  <https://www.ishares.com/gls-download/literature/fact-sheet/ishares-global-aggregate-1-5-year-bond-index-fund-ie-class-d-hedged-eur-factsheet-ie00bmz3nn11-gb-en-individual.pdf>.
- **I2:** BlackRock, iShares product page for the same fund (index ticker I31413US; benchmark "BBG Global Aggregate 1-5 Year Index (USD)"; currency-hedged share classes hedge with derivatives),
  <https://www.ishares.com/uk/individual/en/products/290398>.
- **I3:** BlackRock, *iShares Global Aggregate 1-5 Year Bond Index Fund (IE)* KIID, class D Distributing USD Hedged (index description: multi-currency investment-grade securities with 1–5 year maturities; the share class is hedged with FX forwards),
  <https://www.ishares.com/gls-download/literature/kiid/ucits_kiid-ishares-global-aggregate-1-5-year-bond-index-fund-ie-class-d-hedged-dist-usd-gb-ie00bf2mw460-en.pdf>.
- **P1:** Benchmark-framework session record of 2026-10-17, in which the user approved the stable Ábaco reference as *80% Bloomberg Global Aggregate 1–5 Year total return EUR-hedged / 20% MSCI ACWI Small Cap Value net EUR unhedged* with monthly target resets — retained in the parent research notes, not a public primary document.
- **T1:** Third-party index listing for *Bloomberg Global Aggregate EUR Hedged* (total return, EUR,
  hedged), <https://ycharts.com/indices/%5EBBGATREURH>, accessed 19 October 2026. It is
  discovery of a published variant, not a primary provider document and not a permitted feed.

No source observation was admitted, no production mapping was enabled, and no network call was
made from the test suite.
