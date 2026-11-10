//! The catalogue: its text models read from its JSON, and what a usage costs at a model's prices.

use octoii_ai::catalogue::{self, Conditions, Override, Prices, Pricing, Rate};
use octoii_ai::vocabulary::{ReasoningEffort, Usage};

#[test]
fn the_bundled_catalogue_holds_its_text_models_with_their_facts() {
    let models = catalogue::bundled();
    assert_eq!(models.len(), 58, "the text models alone");
    let haiku = models
        .iter()
        .find(|model| model.reference() == "anthropic/claude-haiku-5-5")
        .expect("listed");
    assert_eq!(haiku.context_length, Some(1_000_000));
    let reasoning = haiku.reasoning.as_ref().expect("it reasons");
    assert_eq!(reasoning.default_effort, Some(ReasoningEffort::Medium));
    assert!(!reasoning.mandatory);
    assert!(haiku.searches_web);
    let pricing = haiku.pricing.as_ref().expect("priced");
    assert_eq!(pricing.base.prompt, Some(Rate::Picodollars(100_000)));
    assert_eq!(pricing.overrides[0].min_prompt_tokens, Some(100_000));
}

#[test]
fn a_reference_is_a_provider_and_an_id_which_may_hold_slashes() {
    assert_eq!(
        catalogue::split_reference("openrouter/anthropic/claude-opus-5-5"),
        Some(("openrouter", "anthropic/claude-opus-5-5"))
    );
    assert_eq!(catalogue::split_reference("/x"), None);
    assert_eq!(catalogue::split_reference("x/"), None);
    assert_eq!(catalogue::split_reference("x"), None);
}

#[test]
fn a_decimal_of_dollars_is_its_picodollars_exactly_and_one_that_varies_is_none() {
    assert_eq!(Rate::of_dollars("0.0000000375"), Rate::Picodollars(37_500));
    assert_eq!(Rate::of_dollars("3"), Rate::Picodollars(3_000_000_000_000));
    assert_eq!(Rate::of_dollars("0.0000000000015"), Rate::Picodollars(2), "a finer digit rounded");
    assert_eq!(Rate::of_dollars("-1"), Rate::Varies);
    assert_eq!(Rate::of_dollars("1e-6"), Rate::Varies);
}

#[test]
fn a_usage_costs_each_meter_at_its_price_and_overrides_by_input_and_time() {
    let dollars = |text| Some(Rate::of_dollars(text));
    let pricing = Pricing {
        base: Prices {
            prompt: dollars("0.000001"),
            completion: dollars("0.000002"),
            cache_read: dollars("0.0000001"),
            web_search: dollars("0.01"),
            ..Prices::default()
        },
        overrides: vec![
            Override {
                min_prompt_tokens: Some(1000),
                prices: Prices {
                    prompt: dollars("0.000003"),
                    ..Prices::default()
                },
                ..Override::default()
            },
            Override {
                utc_days: Some(vec![3]),
                utc_start: Some(2200),
                utc_end: Some(200),
                prices: Prices {
                    completion: dollars("0.000001"),
                    ..Prices::default()
                },
                ..Override::default()
            },
        ],
        discount_millionths: 0,
    };
    // Wed, 28 Oct 2026 12:00 UTC, and Thu, 29 Oct 2026 23:00 UTC.
    let (wednesday, thursday_night) = (1_793_188_800_000, 1_793_314_800_000);
    let usage = Usage {
        input_tokens: 150,
        cached_tokens: 50,
        output_tokens: 10,
        web_searches: 1,
        ..Usage::default()
    };
    let at = |sent_at_ms, context_tokens| Conditions {
        context_tokens,
        sent_at_ms,
    };
    // 100 × 1000 + 50 × 100 + 10 × 2000 nanodollars, and a search's 10,000,000.
    assert_eq!(
        pricing.cost(&usage, at(wednesday, None)),
        Some(100_000 + 5000 + 20_000 + 10_000_000)
    );
    assert_eq!(
        pricing.cost(&usage, at(wednesday, Some(2000))),
        Some(300_000 + 5000 + 20_000 + 10_000_000)
    );
    assert_eq!(
        pricing.cost(&usage, at(thursday_night, None)),
        Some(100_000 + 5000 + 10_000 + 10_000_000)
    );
    let discounted = Pricing {
        discount_millionths: 500_000,
        ..pricing.clone()
    };
    assert_eq!(
        discounted.cost(&usage, at(wednesday, None)),
        Some((100_000 + 5000 + 20_000 + 10_000_000) / 2)
    );
    let varying = Pricing {
        base: Prices {
            completion: Some(Rate::Varies),
            ..pricing.base
        },
        ..pricing.clone()
    };
    assert_eq!(varying.cost(&usage, at(wednesday, None)), None);
    assert_eq!(
        Pricing::default().cost(&usage, at(wednesday, None)),
        None,
        "nothing it counts is priced"
    );
}
