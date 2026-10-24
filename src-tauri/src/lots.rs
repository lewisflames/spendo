//! Cost-basis lot engine for investment returns. Pure: callers load the
//! per-transaction events (fees folded in, converted to the report currency)
//! and this module matches disposals against open lots, FIFO or average cost.
//! Lots are derived on every request, never stored, so they can't go stale
//! after an import or a bulk edit.

/// Share amounts below this are treated as zero (float dust from
/// `quantity_num / quantity_denom`).
const SHARE_EPS: f64 = 1e-9;

/// Money amounts below this count as zero when telling a stock split (no
/// money moved) from a trade.
const VALUE_EPS: f64 = 0.005;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostMethod {
    Fifo,
    Average,
}

impl CostMethod {
    /// Book setting value (`cost_basis_method`); unknown values fall back to FIFO.
    pub fn from_setting(value: &str) -> CostMethod {
        match value {
            "average" => CostMethod::Average,
            _ => CostMethod::Fifo,
        }
    }
}

/// One transaction's net effect on one commodity, in date order.
/// `cost` is the money paid in (fees included), so a buy is positive and a
/// sell's proceeds after fees are `-cost`.
#[derive(Debug, Clone)]
pub struct LotEvent {
    pub date: String,
    pub shares: f64,
    pub cost: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lot {
    pub acquired: String,
    pub shares: f64,
    pub basis: f64,
}

/// Basis matched against one sale. FIFO yields one row per lot consumed;
/// average cost yields one row per sale with no acquisition date.
#[derive(Debug, Clone, PartialEq)]
pub struct Disposal {
    pub date: String,
    pub acquired: Option<String>,
    pub shares: f64,
    pub proceeds: f64,
    pub basis: f64,
}

impl Disposal {
    pub fn gain(&self) -> f64 {
        self.proceeds - self.basis
    }
}

#[derive(Debug, Default)]
pub struct LotSummary {
    pub open_lots: Vec<Lot>,
    pub disposals: Vec<Disposal>,
    /// Set when the history sold more shares than were held; the excess was
    /// treated as sold with zero basis.
    pub warning: Option<String>,
}

impl LotSummary {
    pub fn open_shares(&self) -> f64 {
        self.open_lots.iter().map(|l| l.shares).sum()
    }

    pub fn open_basis(&self) -> f64 {
        self.open_lots.iter().map(|l| l.basis).sum()
    }

    pub fn realized_gain(&self) -> f64 {
        self.disposals.iter().map(Disposal::gain).sum()
    }
}

/// Replay one commodity's events. Same-day buys go before sells so a
/// same-day round trip doesn't read as an oversell.
///
/// - No share change: ignored (transfers between accounts, GnuCash gain
///   splits, zero-amount dividend splits).
/// - Shares added or removed with no money while a position stays open: a
///   stock split, which rescales the open lots and keeps their basis.
/// - Shares added: a buy, opening a lot at `cost`.
/// - Shares removed: a sale for `-cost`, matched FIFO or at average cost. A
///   moneyless removal that closes the position is a write-off at zero.
pub fn replay(events: &[LotEvent], method: CostMethod) -> LotSummary {
    let mut ordered: Vec<&LotEvent> = events.iter().collect();
    ordered.sort_by(|a, b| {
        a.date
            .cmp(&b.date)
            .then_with(|| (a.shares < 0.0).cmp(&(b.shares < 0.0)))
    });

    let mut summary = LotSummary::default();
    for e in ordered {
        if e.shares.abs() < SHARE_EPS {
            continue;
        }
        let open = summary.open_shares();
        let after = open + e.shares;
        if e.cost.abs() < VALUE_EPS && open > SHARE_EPS && after > SHARE_EPS {
            let ratio = after / open;
            for lot in &mut summary.open_lots {
                lot.shares *= ratio;
            }
            continue;
        }
        if e.shares > 0.0 {
            summary.open_lots.push(Lot {
                acquired: e.date.clone(),
                shares: e.shares,
                basis: e.cost,
            });
        } else {
            sell(&mut summary, e, method);
        }
    }
    summary
}

fn sell(summary: &mut LotSummary, e: &LotEvent, method: CostMethod) {
    let sold = -e.shares;
    let proceeds = -e.cost;
    let open = summary.open_shares();
    let matched = sold.min(open.max(0.0));

    if matched > SHARE_EPS {
        match method {
            CostMethod::Fifo => {
                let mut remaining = matched;
                for lot in &mut summary.open_lots {
                    if remaining < SHARE_EPS {
                        break;
                    }
                    let take = remaining.min(lot.shares);
                    let basis = lot.basis * take / lot.shares;
                    summary.disposals.push(Disposal {
                        date: e.date.clone(),
                        acquired: Some(lot.acquired.clone()),
                        shares: take,
                        proceeds: proceeds * take / sold,
                        basis,
                    });
                    lot.shares -= take;
                    lot.basis -= basis;
                    remaining -= take;
                }
            }
            CostMethod::Average => {
                let fraction = matched / open;
                let mut basis = 0.0;
                for lot in &mut summary.open_lots {
                    let part = lot.basis * fraction;
                    basis += part;
                    lot.basis -= part;
                    lot.shares -= lot.shares * fraction;
                }
                summary.disposals.push(Disposal {
                    date: e.date.clone(),
                    acquired: None,
                    shares: matched,
                    proceeds: proceeds * matched / sold,
                    basis,
                });
            }
        }
        summary.open_lots.retain(|l| l.shares >= SHARE_EPS);
    }

    let excess = sold - matched;
    if excess > SHARE_EPS {
        summary.disposals.push(Disposal {
            date: e.date.clone(),
            acquired: None,
            shares: excess,
            proceeds: proceeds * excess / sold,
            basis: 0.0,
        });
        if summary.warning.is_none() {
            summary.warning = Some(format!(
                "Sold more shares than held on {}; the extra counts as zero cost",
                e.date
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(date: &str, shares: f64, cost: f64) -> LotEvent {
        LotEvent {
            date: date.to_string(),
            shares,
            cost,
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn fifo_partial_sale_consumes_oldest_lot_first() {
        let events = [
            ev("2024-01-10", 10.0, 1000.0),
            ev("2024-02-05", 5.0, 550.0),
            ev("2024-03-01", -12.0, -1440.0),
        ];
        let s = replay(&events, CostMethod::Fifo);
        assert_eq!(s.disposals.len(), 2);
        assert_eq!(s.disposals[0].acquired.as_deref(), Some("2024-01-10"));
        assert!(close(s.disposals[0].shares, 10.0));
        assert!(close(s.disposals[0].basis, 1000.0));
        assert!(close(s.disposals[0].proceeds, 1200.0));
        assert_eq!(s.disposals[1].acquired.as_deref(), Some("2024-02-05"));
        assert!(close(s.disposals[1].basis, 220.0));
        assert!(close(s.realized_gain(), 1440.0 - 1220.0));
        assert!(close(s.open_shares(), 3.0));
        assert!(close(s.open_basis(), 330.0));
        assert!(s.warning.is_none());
    }

    #[test]
    fn average_cost_sale_uses_pooled_basis() {
        let events = [
            ev("2024-01-10", 10.0, 1000.0),
            ev("2024-02-05", 5.0, 550.0),
            ev("2024-03-01", -12.0, -1440.0),
        ];
        let s = replay(&events, CostMethod::Average);
        assert_eq!(s.disposals.len(), 1);
        assert_eq!(s.disposals[0].acquired, None);
        // Average cost 1550 / 15 per share.
        assert!(close(s.disposals[0].basis, 1240.0));
        assert!(close(s.realized_gain(), 200.0));
        assert!(close(s.open_shares(), 3.0));
        assert!(close(s.open_basis(), 310.0));
    }

    #[test]
    fn stock_split_rescales_lots_and_keeps_basis() {
        let events = [
            ev("2024-01-10", 10.0, 1000.0),
            ev("2024-06-01", 10.0, 0.0),
            ev("2024-07-01", -5.0, -300.0),
        ];
        let s = replay(&events, CostMethod::Fifo);
        assert!(close(s.disposals[0].basis, 250.0));
        assert!(close(s.realized_gain(), 50.0));
        assert!(close(s.open_shares(), 15.0));
        assert!(close(s.open_basis(), 750.0));
    }

    #[test]
    fn reverse_split_rescales_but_moneyless_close_is_a_write_off() {
        let s = replay(
            &[ev("2024-01-10", 10.0, 1000.0), ev("2024-06-01", -5.0, 0.0)],
            CostMethod::Fifo,
        );
        assert!(s.disposals.is_empty());
        assert!(close(s.open_shares(), 5.0));
        assert!(close(s.open_basis(), 1000.0));

        let s = replay(
            &[ev("2024-01-10", 10.0, 1000.0), ev("2024-06-01", -10.0, 0.0)],
            CostMethod::Fifo,
        );
        assert!(close(s.realized_gain(), -1000.0));
        assert!(s.open_lots.is_empty());
    }

    #[test]
    fn zero_share_events_are_ignored() {
        // A transfer nets to zero shares; a GnuCash gain split moves value only.
        let s = replay(
            &[
                ev("2024-01-10", 10.0, 1000.0),
                ev("2024-02-01", 0.0, 0.0),
                ev("2024-03-01", 0.0, -200.0),
            ],
            CostMethod::Fifo,
        );
        assert!(s.disposals.is_empty());
        assert!(close(s.open_basis(), 1000.0));
    }

    #[test]
    fn same_day_buy_then_sell_is_not_an_oversell() {
        let s = replay(
            &[
                ev("2024-01-10", -10.0, -1100.0),
                ev("2024-01-10", 10.0, 1000.0),
            ],
            CostMethod::Fifo,
        );
        assert!(s.warning.is_none());
        assert!(close(s.realized_gain(), 100.0));
    }

    #[test]
    fn oversell_counts_the_excess_at_zero_basis_and_warns() {
        let s = replay(
            &[
                ev("2024-01-10", 10.0, 1000.0),
                ev("2024-02-01", -15.0, -1500.0),
            ],
            CostMethod::Fifo,
        );
        assert!(close(s.realized_gain(), 1500.0 - 1000.0));
        assert!(s.open_lots.is_empty());
        assert!(s.warning.as_deref().unwrap().contains("2024-02-01"));
    }

    #[test]
    fn closed_position_leaves_only_realized() {
        let s = replay(
            &[
                ev("2024-01-10", 10.0, 1010.0),
                ev("2025-01-10", -10.0, -1190.0),
            ],
            CostMethod::Fifo,
        );
        assert!(s.open_lots.is_empty());
        assert!(close(s.realized_gain(), 180.0));
    }
}
