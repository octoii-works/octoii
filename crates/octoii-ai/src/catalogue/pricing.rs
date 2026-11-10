//! What a request costs at its model's prices: the prices that apply to it, by its input and the time it was sent, and
//! each meter of its usage at its price, the request's own price once. Prices are kept in picodollars a unit, so every
//! price the catalogue lists is a whole number and no cost is a sum of floating rounding. A cost is the provider's or
//! the catalogue's, never a guess: where a price that counts is unknown, so is the cost.

use crate::vocabulary::Usage;

/// A price a unit: in picodollars, or one that varies, as a router's `-1` says, which no cost can be made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rate {
    Picodollars(u64),
    Varies,
}

impl Rate {
    /// The price a US-dollar decimal names, as `0.0000000375`: exact to the picodollar, a finer digit rounded; one that
    /// is no such number varies.
    #[must_use]
    pub fn of_dollars(text: &str) -> Self {
        let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
        let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
        if !digits(whole) || !(fraction.is_empty() || digits(fraction)) {
            return Self::Varies;
        }
        let Ok(whole) = whole.parse::<u64>() else { return Self::Varies };
        let kept: String = fraction.chars().chain(std::iter::repeat('0')).take(12).collect();
        let rounded_up = fraction.as_bytes().get(12).is_some_and(|digit| *digit >= b'5');
        let fraction: u64 = kept.parse().unwrap_or(0) + u64::from(rounded_up);
        whole
            .checked_mul(1_000_000_000_000)
            .and_then(|whole| whole.checked_add(fraction))
            .map_or(Self::Varies, Self::Picodollars)
    }
}

/// The prices a model lists for a text request, each a unit's: a token read, read from the prompt cache, written to
/// it, written, spent reasoning; a web search; the request itself. None, the meter is not charged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Prices {
    pub prompt: Option<Rate>,
    pub completion: Option<Rate>,
    pub cache_read: Option<Rate>,
    pub cache_write: Option<Rate>,
    pub reasoning: Option<Rate>,
    pub web_search: Option<Rate>,
    pub request: Option<Rate>,
}

impl Prices {
    /// These prices, those `over` names in their place.
    fn under(self, over: &Self) -> Self {
        Self {
            prompt: over.prompt.or(self.prompt),
            completion: over.completion.or(self.completion),
            cache_read: over.cache_read.or(self.cache_read),
            cache_write: over.cache_write.or(self.cache_write),
            reasoning: over.reasoning.or(self.reasoning),
            web_search: over.web_search.or(self.web_search),
            request: over.request.or(self.request),
        }
    }
}

/// Prices that replace a model's base prices for a request that meets every condition given: an input of more
/// tokens than `min_prompt_tokens`; a time of day by UTC from `utc_start` to before `utc_end`, as HHMM, a window that
/// may wrap past midnight; days by UTC, Monday 0.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Override {
    pub min_prompt_tokens: Option<u64>,
    pub utc_start: Option<u32>,
    pub utc_end: Option<u32>,
    pub utc_days: Option<Vec<u8>>,
    pub prices: Prices,
}

impl Override {
    fn applies(&self, input_tokens: u64, sent_at_ms: i64) -> bool {
        if self.min_prompt_tokens.is_some_and(|least| input_tokens <= least) {
            return false;
        }
        let seconds = sent_at_ms.div_euclid(1000);
        let days = seconds.div_euclid(86_400);
        // 1970-01-01 was a Thursday.
        let day = u8::try_from((days + 3).rem_euclid(7)).expect("a day of the week");
        if self.utc_days.as_ref().is_some_and(|days| !days.contains(&day)) {
            return false;
        }
        let of_day = seconds.rem_euclid(86_400);
        let time = u32::try_from(of_day / 3600 * 100 + of_day % 3600 / 60).expect("a time of day");
        let (start, end) = (self.utc_start.unwrap_or(0), self.utc_end.unwrap_or(2400));
        // A window that ends before it starts wraps past midnight.
        if start <= end {
            start <= time && time < end
        } else {
            start <= time || time < end
        }
    }
}

/// A model's prices: its base prices, the overrides a later one of which wins, and the fraction taken off every
/// price, in millionths.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pricing {
    pub base: Prices,
    pub overrides: Vec<Override>,
    pub discount_millionths: u32,
}

/// What decides the prices a usage is charged at, besides the usage: the input of the last request of a response
/// continued over several, where it was, and when the request was sent, in milliseconds since 1970.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conditions {
    pub context_tokens: Option<u64>,
    pub sent_at_ms: i64,
}

impl Pricing {
    /// What `usage` cost, in billionths of a US dollar, at the prices that applied to its request: each meter at its
    /// price, the request's own once; none where a price a counting meter needs varies, or where nothing it counts is
    /// priced.
    #[must_use]
    pub fn cost(&self, usage: &Usage, conditions: Conditions) -> Option<u64> {
        let input = conditions.context_tokens.unwrap_or(usage.input_tokens);
        let prices = self
            .overrides
            .iter()
            .filter(|each| each.applies(input, conditions.sent_at_ms))
            .fold(self.base, |prices, each| prices.under(&each.prices));
        let reasoning = if prices.reasoning.is_some() {
            usage.reasoning_tokens.min(usage.output_tokens)
        } else {
            0
        };
        let read = usage
            .input_tokens
            .saturating_sub(usage.cached_tokens)
            .saturating_sub(usage.cache_write_tokens);
        let meters = [
            (prices.prompt, read),
            (prices.cache_read, usage.cached_tokens),
            (prices.cache_write, usage.cache_write_tokens),
            (prices.completion, usage.output_tokens - reasoning),
            (prices.reasoning, reasoning),
            (prices.web_search, usage.web_searches),
            (prices.request, 1),
        ];
        let mut total: u128 = 0;
        let mut charged = false;
        for (rate, amount) in meters {
            let Some(rate) = rate.filter(|_| amount > 0) else { continue };
            let Rate::Picodollars(picodollars) = rate else { return None };
            total += u128::from(amount) * u128::from(picodollars);
            charged |= amount > 0;
        }
        if !charged {
            return None;
        }
        let kept = total * u128::from(1_000_000 - self.discount_millionths.min(1_000_000)) / 1_000_000;
        // Picodollars to nanodollars, to the nearest.
        u64::try_from((kept + 500) / 1000).ok()
    }
}
