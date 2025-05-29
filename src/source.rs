//! This file contains the definition of the [`Source`] trait and some general impls, notably:
//! - Immutable references: `impl<S> Source for &S`
//! - Tuples up to size 5: `impl<S1, S2, S3, ...> Source for (S1, S2, S3, ...)`
//! - Arrays: `impl<S, const N> Source for [S; N]`
//! - and slices: `impl<S> Source for [S]`
//! All the above assume `S: Source`.
use std::marker::PhantomData;

use seq_macro::seq;

pub trait Source<T> {
    fn get(&mut self) -> Option<T>;

    fn get_existing(&self) -> T;

    fn reset(&mut self);

    fn map<U>(self, map_fn: impl Fn(T) -> U) -> impl Source<U>
    where
        Self: Sized,
    {
        MappedSource {
            source: self,
            map_fn,
            _tys: PhantomData,
        }
    }
}

impl<S1, S2, T1, T2> Source<(T1, T2)> for (S1, S2)
where
    S1: Source<T1>,
    S2: Source<T2>,
{
    #[inline]
    fn get(&mut self) -> Option<(T1, T2)> {
        match (self.0.get(), self.1.get()) {
            (Some(x1), Some(x2)) => Some((x1, x2)),
            (Some(x1), None) => Some((x1, self.1.get_existing())),
            (None, Some(x2)) => Some((self.0.get_existing(), x2)),
            (None, None) => None,
        }
    }

    #[inline]
    fn get_existing(&self) -> (T1, T2) {
        (self.0.get_existing(), self.1.get_existing())
    }

    #[inline]
    fn reset(&mut self) {
        self.0.reset();
        self.1.reset();
    }
}

/// Generate `Source` for an N-element tuple of sources.
macro_rules! impl_source_for_tuple {
    ($n:literal) => {
        seq!(I in 0..$n {
            impl< #( S~I, T~I, )* > Source<(#( T~I, )*)> for (#( S~I, )*)
            where #( S~I: Source<T~I>, )*
            {
                #[inline]
                fn get(&mut self) -> Option<( #(T~I, )*)> {
                    let (#(s~I, )*) = self;

                    // Call `get` on every inner source …
                    #( let o~I = s~I.get(); )*

                    // …and see whether at least one of them produced `Some`.
                    if #(o~I.is_some() || )* false {
                        Some((
                            #( o~I.unwrap_or_else(|| s~I.get_existing()), )*
                        ))
                    } else {
                        None
                    }
                }

                #[inline]
                fn get_existing(&self) -> (#( T~I ,)*) {
                    let (#(s~I, )*) = self;
                    (#(s~I.get_existing(), )*)
                }

                #[inline]
                fn reset(&mut self) {
                    let (#(s~I, )*) = self;
                    #(s~I.reset(); )*
                }
            }
        });
    };
}

impl_source_for_tuple!(3);
impl_source_for_tuple!(4);
impl_source_for_tuple!(5);
impl_source_for_tuple!(6);
impl_source_for_tuple!(7);
impl_source_for_tuple!(8);

impl<S, T, const N: usize> Source<[T; N]> for [S; N]
where
    S: Source<T>,
{
    fn get(&mut self) -> Option<[T; N]> {
        let mut found_updated = false;
        let vals_and_sources: [(Option<T>, &mut S); N] = self.each_mut().map(|s| {
            let val = s.get();
            if val.is_some() {
                found_updated = true;
            }
            (val, s)
        });
        if found_updated {
            Some(vals_and_sources.map(|(val, s)| val.unwrap_or_else(|| s.get_existing())))
        } else {
            None
        }
    }

    fn get_existing(&self) -> [T; N] {
        self.each_ref().map(Source::get_existing)
    }

    fn reset(&mut self) {
        for source in self.iter_mut() {
            source.reset();
        }
    }
}

impl<S, T> Source<Vec<T>> for [S]
where
    S: Source<T> + Clone,
{
    fn get(&mut self) -> Option<Vec<T>> {
        let mut found_updated = false;
        let vals: Vec<Option<T>> = self
            .iter_mut()
            .map(|s| {
                let val = s.get();
                if val.is_some() {
                    found_updated = true;
                }
                val
            })
            .collect();
        if found_updated {
            Some(
                vals.into_iter()
                    .enumerate()
                    .map(|(i, val)| val.unwrap_or_else(|| self[i].get_existing()))
                    .collect(),
            )
        } else {
            None
        }
    }

    fn get_existing(&self) -> Vec<T> {
        self.iter().map(Source::get_existing).collect()
    }

    fn reset(&mut self) {
        for source in self.iter_mut() {
            source.reset();
        }
    }
}

pub struct MappedSource<S, T, U, F> {
    source: S,
    map_fn: F,
    _tys: PhantomData<(T, U)>,
}

impl<S, F, T, U> Source<U> for MappedSource<S, T, U, F>
where
    S: Source<T>,
    F: Fn(T) -> U,
{
    fn get(&mut self) -> Option<U> {
        self.source.get().map(&self.map_fn)
    }

    fn get_existing(&self) -> U {
        (self.map_fn)(self.source.get_existing())
    }

    fn reset(&mut self) {
        self.source.reset()
    }
}
