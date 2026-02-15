# dhan-rs — Implementation Plan

## Overview

A Rust client library for the [DhanHQ Broker API v2](https://dhanhq.co/docs/v2/).
Base URL: `https://api.dhan.co/v2/`
Auth Base URL: `https://auth.dhan.co`

All REST endpoints use JSON. WebSocket feeds use binary (little-endian) for market data and JSON for order updates.

---

## Crate Dependencies

| Crate | Purpose |
|---|---|
| `reqwest` | HTTP client (async, JSON, TLS) |
| `serde` / `serde_json` | JSON serialization & deserialization |
| `tokio` | Async runtime |
| `tokio-tungstenite` | WebSocket client (market feed + order updates) |
| `thiserror` | Ergonomic error types |
| `secrecy` | Wrapping secrets (access tokens, API keys) |
| `chrono` | Date/time handling |
| `tracing` | Structured logging |
| `byteorder` | Parsing little-endian binary market feed packets |
| `url` | URL construction |

---

## Module / File Structure

```
src/
├── lib.rs                  # Re-exports, DhanClient builder
├── client.rs               # Core HTTP client (reqwest wrapper, auth headers, rate limiting)
├── error.rs                # Error types (DH-901..DH-910, network, parse errors)
├── types/
│   ├── mod.rs
│   ├── enums.rs            # All shared enums (ExchangeSegment, ProductType, OrderType, etc.)
│   ├── auth.rs             # Auth request/response types
│   ├── orders.rs           # Order placement/modification/book/trade types
│   ├── super_order.rs      # Super Order types (with leg details)
│   ├── forever_order.rs    # Forever Order / GTT types
│   ├── conditional.rs      # Conditional Trigger types (conditions, indicators, operators)
│   ├── portfolio.rs        # Holdings, Positions, ConvertPosition types
│   ├── edis.rs             # EDIS types (form, inquiry)
│   ├── traders_control.rs  # Kill switch, P&L-based exit types
│   ├── funds.rs            # Fund limit, margin calculator types
│   ├── statements.rs       # Ledger report, trade history types
│   ├── market_quote.rs     # LTP, OHLC, market depth quote types
│   ├── historical.rs       # Daily & intraday candle types
│   ├── option_chain.rs     # Option chain, expiry list, greeks types
│   ├── postback.rs         # Postback (webhook) payload types
│   ├── ip.rs               # Static IP management types
│   └── profile.rs          # User profile types
├── api/
│   ├── mod.rs
│   ├── auth.rs             # Authentication endpoints
│   ├── orders.rs           # Order CRUD + slicing
│   ├── super_order.rs      # Super Order CRUD
│   ├── forever_order.rs    # Forever Order CRUD
│   ├── conditional.rs      # Conditional Trigger CRUD
│   ├── portfolio.rs        # Holdings, Positions, Convert, Exit All
│   ├── edis.rs             # EDIS T-PIN, form, inquiry
│   ├── traders_control.rs  # Kill switch, P&L exit
│   ├── funds.rs            # Margin calculator, fund limit
│   ├── statements.rs       # Ledger, trade history
│   ├── market_quote.rs     # LTP, OHLC, quote snapshots
│   ├── historical.rs       # Daily & intraday historical data
│   ├── option_chain.rs     # Option chain, expiry list
│   └── ip.rs               # Static IP set/modify/get
├── ws/
│   ├── mod.rs
│   ├── market_feed.rs      # Live Market Feed WebSocket (binary parsing)
│   └── order_update.rs     # Live Order Update WebSocket (JSON)
└── constants.rs            # Base URLs, API paths, rate limit values
```

---

## Implementation Phases

### Phase 1: Foundation

**Step 1 — Error types** (`src/error.rs`)
- Define `DhanError` enum covering:
  - API errors with `errorType`, `errorCode`, `errorMessage` (DH-901 to DH-910)
  - HTTP errors (status codes, network)
  - Deserialization errors
  - WebSocket errors
- Implement `std::error::Error`, `Display`, `From<reqwest::Error>`, etc.

**Step 2 — Shared enums** (`src/types/enums.rs`)
- `ExchangeSegment`: `IDX_I`, `NSE_EQ`, `NSE_FNO`, `NSE_CURRENCY`, `BSE_EQ`, `MCX_COMM`, `BSE_CURRENCY`, `BSE_FNO`
- `TransactionType`: `BUY`, `SELL`
- `ProductType`: `CNC`, `INTRADAY`, `MARGIN`, `MTF`, `CO`, `BO`
- `OrderType`: `LIMIT`, `MARKET`, `STOP_LOSS`, `STOP_LOSS_MARKET`
- `OrderStatus`: `TRANSIT`, `PENDING`, `CLOSED`, `TRIGGERED`, `REJECTED`, `CANCELLED`, `PART_TRADED`, `TRADED`, `EXPIRED`, `CONFIRM`
- `Validity`: `DAY`, `IOC`
- `LegName`: `ENTRY_LEG`, `TARGET_LEG`, `STOP_LOSS_LEG`
- `PositionType`: `LONG`, `SHORT`, `CLOSED`
- `OptionType`: `CALL`, `PUT`
- `AmoTime`: `PRE_OPEN`, `OPEN`, `OPEN_30`, `OPEN_60`
- `Instrument`: `INDEX`, `FUTIDX`, `OPTIDX`, `EQUITY`, `FUTSTK`, `OPTSTK`, `FUTCOM`, `OPTFUT`, `FUTCUR`, `OPTCUR`
- `ExpiryCode`: `Near(0)`, `Next(1)`, `Far(2)`
- `OrderFlag`: `SINGLE`, `OCO`
- `KillSwitchStatus`: `ACTIVATE`, `DEACTIVATE`
- `IpFlag`: `PRIMARY`, `SECONDARY`
- `FeedRequestCode`: `Connect(11)`, `Disconnect(12)`, `SubscribeTicker(15)`, etc.
- `FeedResponseCode`: `Index(1)`, `Ticker(2)`, `Quote(4)`, `OI(5)`, `PrevClose(6)`, `MarketStatus(7)`, `Full(8)`, `Disconnect(50)`
- Conditional trigger enums: `ComparisonType`, `IndicatorName`, `Operator`, `AlertStatus`

**Step 3 — Constants** (`src/constants.rs`)
- Base URLs: `https://api.dhan.co/v2`, `https://auth.dhan.co`
- WebSocket URLs: `wss://api-feed.dhan.co`, `wss://api-order-update.dhan.co`
- Rate limits (per second / minute / hour / day)

**Step 4 — HTTP Client** (`src/client.rs`)
- `DhanClient` struct holding:
  - `reqwest::Client`
  - `access_token: String`
  - `client_id: String`
- Builder pattern for construction
- Generic methods: `get()`, `post()`, `put()`, `delete()` that inject:
  - `access-token` header
  - `Content-Type: application/json`
  - Deserialize response or return `DhanError`
- Rate limiting awareness (optional: token bucket / leaky bucket)

---

### Phase 2: Authentication & Profile

**Step 5 — Auth types** (`src/types/auth.rs`)
- `GenerateTokenResponse { dhanClientId, dhanClientName, dhanClientUcc, givenPowerOfAttorney, accessToken, expiryTime }`
- `ConsentResponse { consentAppId/consentId, consentAppStatus/consentStatus }`

**Step 6 — Auth API** (`src/api/auth.rs`)
- `generate_access_token(client_id, pin, totp)` → POST `auth.dhan.co/app/generateAccessToken`
- `renew_token()` → GET `/v2/RenewToken`
- API key flow (3 steps): `generate_consent()`, browser redirect URL builder, `consume_consent(token_id)`
- Partner flow (3 steps): `partner_generate_consent()`, browser redirect URL builder, `partner_consume_consent(token_id)`

**Step 7 — Profile & IP types + API** (`src/types/profile.rs`, `src/types/ip.rs`, `src/api/ip.rs`)
- `UserProfile { dhanClientId, tokenValidity, activeSegment, ddpi, mtf, dataPlan, dataValidity }`
- `get_profile()` → GET `/v2/profile`
- `IpInfo`, `set_ip()`, `modify_ip()`, `get_ip()`

---

### Phase 3: Order Management

**Step 8 — Order types** (`src/types/orders.rs`)
- `PlaceOrderRequest { dhanClientId, correlationId, transactionType, exchangeSegment, productType, orderType, validity, securityId, quantity, disclosedQuantity, price, triggerPrice, afterMarketOrder, amoTime, boProfitValue, boStopLossValue }`
- `OrderResponse { orderId, orderStatus }`
- `ModifyOrderRequest { dhanClientId, orderId, orderType, legName, quantity, price, disclosedQuantity, triggerPrice, validity }`
- `OrderDetail` (full order book entry with all fields)
- `TradeDetail` (trade book entry)

**Step 9 — Orders API** (`src/api/orders.rs`)
- `place_order(req)` → POST `/v2/orders`
- `modify_order(order_id, req)` → PUT `/v2/orders/{order-id}`
- `cancel_order(order_id)` → DELETE `/v2/orders/{order-id}`
- `slice_order(req)` → POST `/v2/orders/slicing`
- `get_orders()` → GET `/v2/orders`
- `get_order(order_id)` → GET `/v2/orders/{order-id}`
- `get_order_by_correlation_id(correlation_id)` → GET `/v2/orders/external/{correlation-id}`
- `get_trades()` → GET `/v2/trades`
- `get_trades_for_order(order_id)` → GET `/v2/trades/{order-id}`

---

### Phase 4: Super Orders

**Step 10 — Super Order types** (`src/types/super_order.rs`)
- `PlaceSuperOrderRequest { ..., targetPrice, stopLossPrice, trailingJump }`
- `ModifySuperOrderRequest { ..., legName, targetPrice, stopLossPrice, trailingJump }`
- `SuperOrderDetail` (with nested `legDetails` array)
- `LegDetail { orderId, legName, transactionType, totalQuantity, remainingQuantity, triggeredQuantity, price, orderStatus, trailingJump }`

**Step 11 — Super Orders API** (`src/api/super_order.rs`)
- `place_super_order(req)` → POST `/v2/super/orders`
- `modify_super_order(order_id, req)` → PUT `/v2/super/orders/{order-id}`
- `cancel_super_order(order_id, leg)` → DELETE `/v2/super/orders/{order-id}/{order-leg}`
- `get_super_orders()` → GET `/v2/super/orders`

---

### Phase 5: Forever Orders

**Step 12 — Forever Order types** (`src/types/forever_order.rs`)
- `CreateForeverOrderRequest { ..., orderFlag, price, triggerPrice, price1, triggerPrice1, quantity1 }`
- `ModifyForeverOrderRequest { ..., orderFlag, legName, quantity, price, triggerPrice }`
- `ForeverOrderDetail`

**Step 13 — Forever Orders API** (`src/api/forever_order.rs`)
- `create_forever_order(req)` → POST `/v2/forever/orders`
- `modify_forever_order(order_id, req)` → PUT `/v2/forever/orders/{order-id}`
- `delete_forever_order(order_id)` → DELETE `/v2/forever/orders/{order-id}`
- `get_all_forever_orders()` → GET `/v2/forever/all`

---

### Phase 6: Conditional Triggers

**Step 14 — Conditional Trigger types** (`src/types/conditional.rs`)
- `AlertCondition { comparisonType, exchangeSegment, securityId, indicatorName, timeFrame, operator, comparingValue, comparingIndicatorName, expDate, frequency, userNote }`
- `AlertOrder { transactionType, exchangeSegment, productType, orderType, securityId, quantity, validity, price, discQuantity, triggerPrice }`
- `PlaceConditionalTriggerRequest { dhanClientId, condition, orders }`
- `ConditionalTriggerDetail { alertId, alertStatus, createdTime, triggeredTime, lastPrice, condition, orders }`

**Step 15 — Conditional Triggers API** (`src/api/conditional.rs`)
- `place_conditional_trigger(req)` → POST `/v2/alerts/orders`
- `modify_conditional_trigger(alert_id, req)` → PUT `/v2/alerts/orders/{alertId}`
- `delete_conditional_trigger(alert_id)` → DELETE `/v2/alerts/orders/{alertId}`
- `get_conditional_trigger(alert_id)` → GET `/v2/alerts/orders/{alertId}`
- `get_all_conditional_triggers()` → GET `/v2/alerts/orders`

---

### Phase 7: Portfolio & Positions

**Step 16 — Portfolio types** (`src/types/portfolio.rs`)
- `Holding { exchange, tradingSymbol, securityId, isin, totalQty, dpQty, t1Qty, availableQty, collateralQty, avgCostPrice }`
- `Position { dhanClientId, tradingSymbol, securityId, positionType, exchangeSegment, productType, buyAvg, buyQty, costPrice, sellAvg, sellQty, netQty, realizedProfit, unrealizedProfit, rbiReferenceRate, multiplier, carryForwardBuyQty, carryForwardSellQty, carryForwardBuyValue, carryForwardSellValue, dayBuyQty, daySellQty, dayBuyValue, daySellValue, drvExpiryDate, drvOptionType, drvStrikePrice, crossCurrency }`
- `ConvertPositionRequest { dhanClientId, fromProductType, exchangeSegment, positionType, securityId, tradingSymbol, convertQty, toProductType }`

**Step 17 — Portfolio API** (`src/api/portfolio.rs`)
- `get_holdings()` → GET `/v2/holdings`
- `get_positions()` → GET `/v2/positions`
- `convert_position(req)` → POST `/v2/positions/convert`
- `exit_all_positions()` → DELETE `/v2/positions`

---

### Phase 8: EDIS

**Step 18 — EDIS types + API** (`src/types/edis.rs`, `src/api/edis.rs`)
- `EdisFormRequest { isin, qty, exchange, segment, bulk }`
- `EdisFormResponse { dhanClientId, edisFormHtml }`
- `EdisInquiry { clientId, isin, totalQty, aprvdQty, status, remarks }`
- `generate_tpin()` → GET `/v2/edis/tpin`
- `generate_edis_form(req)` → POST `/v2/edis/form`
- `inquire_edis(isin)` → GET `/v2/edis/inquire/{isin}`

---

### Phase 9: Trader's Control

**Step 19 — Trader's Control types + API** (`src/types/traders_control.rs`, `src/api/traders_control.rs`)
- `KillSwitchResponse { dhanClientId, killSwitchStatus }`
- `PnlExitRequest { profitValue, lossValue, productType, enableKillSwitch }`
- `PnlExitResponse { pnlExitStatus, message }`
- `PnlExitConfig { pnlExitStatus, profit, loss, segments, enableKillSwitch }`
- `manage_kill_switch(status)` → POST `/v2/killswitch?killSwitchStatus={status}`
- `get_kill_switch_status()` → GET `/v2/killswitch`
- `set_pnl_exit(req)` → POST `/v2/pnlExit`
- `stop_pnl_exit()` → DELETE `/v2/pnlExit`
- `get_pnl_exit()` → GET `/v2/pnlExit`

---

### Phase 10: Funds & Margin

**Step 20 — Funds types + API** (`src/types/funds.rs`, `src/api/funds.rs`)
- `MarginCalculatorRequest { dhanClientId, exchangeSegment, transactionType, quantity, productType, securityId, price, triggerPrice }`
- `MarginCalculatorResponse { totalMargin, spanMargin, exposureMargin, availableBalance, variableMargin, insufficientBalance, brokerage, leverage }`
- `MultiMarginRequest { includePosition, includeOrders, scripts: Vec<MarginScript> }`
- `FundLimit { dhanClientId, availabelBalance, sodLimit, collateralAmount, receiveableAmount, utilizedAmount, blockedPayoutAmount, withdrawableBalance }`
- `calculate_margin(req)` → POST `/v2/margincalculator`
- `calculate_multi_margin(req)` → POST `/v2/margincalculator/multi`
- `get_fund_limit()` → GET `/v2/fundlimit`

---

### Phase 11: Statements

**Step 21 — Statement types + API** (`src/types/statements.rs`, `src/api/statements.rs`)
- `LedgerEntry { dhanClientId, narration, voucherdate, exchange, voucherdesc, vouchernumber, debit, credit, runbal }`
- `TradeHistoryEntry { ..., isin, instrument, sebiTax, stt, brokerageCharges, serviceTax, exchangeTransactionCharges, stampDuty, ... }`
- `get_ledger(from_date, to_date)` → GET `/v2/ledger?from-date={}&to-date={}`
- `get_trade_history(from_date, to_date, page)` → GET `/v2/trades/{from-date}/{to-date}/{page}`

---

### Phase 12: Market Quotes (REST)

**Step 22 — Market Quote types + API** (`src/types/market_quote.rs`, `src/api/market_quote.rs`)
- `MarketQuoteRequest` — HashMap of `ExchangeSegment → Vec<SecurityId>`
- `TickerResponse` (LTP per security)
- `OhlcResponse` (LTP + OHLC per security)
- `MarketDepthResponse` (full quote: depth, OI, volume, circuit limits, etc.)
- `get_ltp(req)` → POST `/v2/marketfeed/ltp`
- `get_ohlc(req)` → POST `/v2/marketfeed/ohlc`
- `get_quote(req)` → POST `/v2/marketfeed/quote`

---

### Phase 13: Historical Data

**Step 23 — Historical Data types + API** (`src/types/historical.rs`, `src/api/historical.rs`)
- `HistoricalDataRequest { securityId, exchangeSegment, instrument, expiryCode, oi, fromDate, toDate }`
- `IntradayDataRequest { ..., interval (1/5/15/25/60) }`
- `CandleData { open: Vec<f64>, high: Vec<f64>, low: Vec<f64>, close: Vec<f64>, volume: Vec<i64>, timestamp: Vec<i64>, open_interest: Vec<i64> }`
- `get_daily_historical(req)` → POST `/v2/charts/historical`
- `get_intraday_historical(req)` → POST `/v2/charts/intraday`

---

### Phase 14: Option Chain

**Step 24 — Option Chain types + API** (`src/types/option_chain.rs`, `src/api/option_chain.rs`)
- `OptionChainRequest { UnderlyingScrip, UnderlyingSeg, Expiry }`
- `OptionChainResponse { last_price, oc: HashMap<String, StrikeData> }` where `StrikeData { ce, pe }`
- `OptionData { average_price, greeks: Greeks, implied_volatility, last_price, oi, previous_close_price, previous_oi, previous_volume, security_id, top_ask_price, top_ask_quantity, top_bid_price, top_bid_quantity, volume }`
- `Greeks { delta, theta, gamma, vega }`
- `ExpiryListRequest { UnderlyingScrip, UnderlyingSeg }`
- `get_option_chain(req)` → POST `/v2/optionchain`
- `get_expiry_list(req)` → POST `/v2/optionchain/expirylist`

---

### Phase 15: Postback (Webhook Deserialization)

**Step 25 — Postback types** (`src/types/postback.rs`)
- `PostbackPayload` — struct matching the webhook JSON body (same shape as `OrderDetail` + `filled_qty`, `algoId`)
- Provide `From<PostbackPayload>` conversions or helper methods
- This module is for users who want to **receive** postback webhook POSTs in their own server — we just provide the type for deserialization

---

### Phase 16: WebSocket — Live Order Updates

**Step 26 — Order Update WebSocket** (`src/ws/order_update.rs`)
- Connection to `wss://api-order-update.dhan.co`
- Auth message: `{ LoginReq: { MsgCode: 42, ClientId, Token }, UserType: "SELF" }`
- Partner variant: `{ LoginReq: { MsgCode: 42, ClientId }, UserType: "PARTNER", Secret }`
- Parse incoming JSON `OrderUpdateMessage` with `Type: "order_alert"` and `Data: { ... }`
- Expose as `Stream<Item = OrderUpdateMessage>` or callback-based API
- Auto-reconnect + heartbeat handling

---

### Phase 17: WebSocket — Live Market Feed

**Step 27 — Market Feed WebSocket** (`src/ws/market_feed.rs`)
- Connection to `wss://api-feed.dhan.co?version=2&token={}&clientId={}&authType=2`
- Subscribe/unsubscribe via JSON: `{ RequestCode, InstrumentCount, InstrumentList }`
- Parse **binary** response packets (little-endian):
  - **Response Header** (8 bytes): response code (1B), message length (2B), exchange segment (1B), security ID (4B)
  - **Ticker Packet** (code 2): LTP (f32) + LTT (i32)
  - **Prev Close Packet** (code 6): prev close (f32) + prev OI (i32)
  - **Quote Packet** (code 4): LTP, last qty, LTT, ATP, volume, sell qty, buy qty, OHLC
  - **OI Packet** (code 5): OI (i32)
  - **Full Packet** (code 8): Quote data + OI + market depth (5 × 20-byte packets: bid qty, ask qty, bid orders, ask orders, bid price, ask price)
  - **Disconnect Packet** (code 50): disconnect reason code (i16)
- Expose as `Stream<Item = MarketFeedEvent>` or callback-based API
- Ping/pong keepalive (server pings every 10s, must respond within 40s)
- Max 5 connections per user, 5000 instruments per connection, 100 instruments per subscribe message

---

### Phase 18: Testing & Polish

**Step 28 — Unit tests**
- Serde round-trip tests for all request/response types
- Binary packet parsing tests for market feed
- Error deserialization tests

**Step 29 — Integration test harness**
- Mock server or recorded HTTP fixtures
- WebSocket mock for market feed + order updates

**Step 30 — Documentation & examples**
- Rustdoc on all public types and methods
- `examples/` directory:
  - `place_order.rs` — basic order placement
  - `market_feed.rs` — subscribe to live market data
  - `option_chain.rs` — fetch option chain
  - `portfolio.rs` — check holdings & positions

**Step 31 — CI / publish prep**
- GitHub Actions workflow
- `README.md` with usage examples
- Publish to crates.io

---

## Rate Limit Summary

| Category | Per Second | Per Minute | Per Hour | Per Day |
|---|---|---|---|---|
| Orders | 10 | 250 | 1000 | 7000 |
| Data (REST) | 5 | — | — | 100,000 |
| Historical | 1 | Unlimited | Unlimited | Unlimited |
| Instruments | 20 | Unlimited | Unlimited | Unlimited |

Order Modifications: max 25 per order.
Option Chain: 1 request per 3 seconds.
Market Quote (REST): up to 1000 instruments per request, 1 req/sec.
WebSocket: up to 5 connections, 5000 instruments each, 100 per subscribe message.
