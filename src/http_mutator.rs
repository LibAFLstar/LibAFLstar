//! Extra mutators that are specific to a target.

use std::marker::PhantomData;

use libafl::{
    inputs::HasBytesVec,
    mutators::{MutationResult, Mutator},
};
use libafl_bolts::{prelude::Error, Named};

/// Mutator that simply appends `\r\n` to each test case.
/// This is required by the LightFTP parser.
pub struct HttpMutator<M, I, S>
where
    M: Mutator<I, S>,
{
    name: String,
    inner: M,
    phantom: PhantomData<(I, S)>,
}

impl<M, I, S> HttpMutator<M, I, S>
where
    M: Mutator<I, S>,
{
    pub fn new(mutator: M) -> Self {
        Self {
            name: format!("HttpMutator[{}]", mutator.name()),
            inner: mutator,
            phantom: PhantomData,
        }
    }
}

impl<M, I, S> Mutator<I, S> for HttpMutator<M, I, S>
where
    M: Mutator<I, S>,
    I: HasBytesVec,
{
    fn mutate(
        &mut self,
        state: &mut S,
        input: &mut I,
        stage_idx: i32,
    ) -> Result<MutationResult, Error> {
        match self.inner.mutate(state, input, stage_idx)? {
            m @ MutationResult::Mutated => {
                let v = input.bytes_mut();
                v.push(b'\r');
                v.push(b'\n');
                v.push(b'\r');
                v.push(b'\n');
                Ok(m)
            }
            s @ MutationResult::Skipped => Ok(s),
        }
    }
}

impl<M, I, S> Named for HttpMutator<M, I, S>
where
    M: Mutator<I, S>,
{
    fn name(&self) -> &str {
        &self.name
    }
}
