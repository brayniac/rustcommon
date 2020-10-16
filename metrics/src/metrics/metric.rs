// Copyright 2020 Twitter, Inc.
// Licensed under the Apache License, Version 2.0
// http://www.apache.org/licenses/LICENSE-2.0

use crate::entry::Entry;
use crate::outputs::ApproxOutput;
use crate::*;
use core::hash::Hash;
use core::hash::Hasher;

use rustcommon_atomics::*;


/// A statistic and output pair which has a corresponding value
// #[derive(PartialEq, Eq, Hash)]
pub struct Metric<Value, Count>
where
    Value: crate::Value,
    Count: crate::Count,
    <Value as Atomic>::Primitive: Primitive,
    <Count as Atomic>::Primitive: Primitive,
    u64: From<<Value as Atomic>::Primitive> + From<<Count as Atomic>::Primitive>,
{
    pub(crate) statistic: Entry<Value, Count>,
    pub(crate) output: ApproxOutput,
}

impl<Value, Count> Hash for Metric<Value, Count>
where
    Value: crate::Value,
    Count: crate::Count,
    <Value as Atomic>::Primitive: Primitive,
    <Count as Atomic>::Primitive: Primitive,
    u64: From<<Value as Atomic>::Primitive> + From<<Count as Atomic>::Primitive>,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.statistic.name().hash(state);
        self.output.hash(state);
    }
}

impl<Value, Count> PartialEq for Metric<Value, Count>
where
    Value: crate::Value,
    Count: crate::Count,
    <Value as Atomic>::Primitive: Primitive,
    <Count as Atomic>::Primitive: Primitive,
    u64: From<<Value as Atomic>::Primitive> + From<<Count as Atomic>::Primitive>,
{
    fn eq(&self, other: &Self) -> bool {
        self.statistic.name() == other.statistic.name() && self.output == other.output
    }
}

impl<Value, Count> Eq for Metric<Value, Count>
where
    Value: crate::Value,
    Count: crate::Count,
    <Value as Atomic>::Primitive: Primitive,
    <Count as Atomic>::Primitive: Primitive,
    u64: From<<Value as Atomic>::Primitive> + From<<Count as Atomic>::Primitive>,
{
}

impl<Value, Count> Metric<Value, Count>
where
    Value: crate::Value,
    Count: crate::Count,
    <Value as Atomic>::Primitive: Primitive,
    <Count as Atomic>::Primitive: Primitive,
    u64: From<<Value as Atomic>::Primitive> + From<<Count as Atomic>::Primitive>,
{
    /// Get the statistic name for the metric
    pub fn statistic(&self) -> &dyn Statistic<Value, Count> {
        &self.statistic as &dyn Statistic<Value, Count>
    }

    /// Get the output
    pub fn output(&self) -> Output {
        Output::from(self.output)
    }
}