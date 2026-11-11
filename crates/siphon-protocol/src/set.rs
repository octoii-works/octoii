//! A set of methods: those of Siphon's core, or of a plugin — declared once by `methods!`, read in Rust as the set's
//! own `Method`, and described in TypeScript with the events and topics of its subscriptions, so a host that
//! assembles sets gives its clients the protocol they make together.

use ts_rs::Config;

use crate::Scope;
use crate::typescript::Declarations;

/// A set of methods as a host assembles it: each method's name and the scope a session needs to call it; the
/// TypeScript names of each one's params and result; a visit of every type the set names; and the TypeScript names of
/// its subscriptions' events and topics, where it has subscriptions.
#[derive(Debug, Clone)]
pub struct Set {
    pub methods: Vec<(&'static str, Option<Scope>)>,
    pub signatures: fn(&Config) -> Vec<(&'static str, String, String)>,
    pub visit: fn(&mut Declarations<'_>),
    pub subscriptions: Option<Subscriptions>,
}

/// The TypeScript names of a set's subscriptions' events and topics.
#[derive(Debug, Clone, Copy)]
pub struct Subscriptions {
    pub event: fn(&Config) -> String,
    pub topic: fn(&Config) -> String,
}

/// A set of methods, as `Method` and `set()` in the module it is written in, whose crate depends on `serde` as its types
/// do: the events and topics of its subscriptions,
/// then each method — its name, the variant of `Method` it is read as, its params, its result and the scope a session
/// needs to call it, none where its params decide.
#[macro_export]
macro_rules! methods {
    (
        events: $event:ty, topics: $topic:ty;
        $($(#[$doc:meta])* $name:literal $variant:ident($params:ty) -> $result:ty, $scope:expr;)*
    ) => {
        $crate::methods! { @set
            (Some($crate::Subscriptions {
                event: |cfg| <$event as $crate::ts_rs::TS>::name(cfg),
                topic: |cfg| <$topic as $crate::ts_rs::TS>::name(cfg),
            }))
            (|declarations| {
                $crate::ts_rs::TypeVisitor::visit::<$event>(declarations);
                $crate::ts_rs::TypeVisitor::visit::<$topic>(declarations);
            });
            $($(#[$doc])* $name $variant($params) -> $result, $scope;)*
        }
    };
    (
        $($(#[$doc:meta])* $name:literal $variant:ident($params:ty) -> $result:ty, $scope:expr;)*
    ) => {
        $crate::methods! { @set (None) (|_| {}); $($(#[$doc])* $name $variant($params) -> $result, $scope;)* }
    };
    (
        @set ($subscriptions:expr) ($visit_subscriptions:expr);
        $($(#[$doc:meta])* $name:literal $variant:ident($params:ty) -> $result:ty, $scope:expr;)*
    ) => {
        /// A call's method, with its params.
        #[derive(Debug, Clone, PartialEq, ::serde::Deserialize)]
        #[serde(tag = "method", content = "params")]
        pub enum Method {
            $($(#[$doc])* #[serde(rename = $name)] $variant($params),)*
        }

        impl Method {
            /// The method `name` with `params`, as a call gives them.
            ///
            /// # Errors
            /// Where the set has no such method, or its params are not the method's.
            pub fn read(name: &str, params: $crate::serde_json::Value) -> $crate::serde_json::Result<Self> {
                $crate::serde_json::from_value($crate::serde_json::json!({ "method": name, "params": params }))
            }

            /// Its name, as a call names it.
            #[must_use]
            pub fn name(&self) -> &'static str {
                match self {
                    $(Self::$variant(_) => $name,)*
                }
            }
        }

        /// The set, as a host assembles it.
        #[must_use]
        pub fn set() -> $crate::Set {
            $crate::Set {
                methods: vec![$(($name, $scope)),*],
                signatures: |cfg| vec![$(($name, <$params as $crate::ts_rs::TS>::name(cfg), <$result as $crate::ts_rs::TS>::name(cfg)),)*],
                visit: |declarations| {
                    $(
                        $crate::ts_rs::TypeVisitor::visit::<$params>(declarations);
                        $crate::ts_rs::TypeVisitor::visit::<$result>(declarations);
                    )*
                    let visit_subscriptions: fn(&mut $crate::Declarations<'_>) = $visit_subscriptions;
                    visit_subscriptions(declarations);
                },
                subscriptions: $subscriptions,
            }
        }
    };
}
