//! This file contains the definition of the [`Source`] trait and some general impls, notably:
//! - Immutable references: `impl<S> Source for &S`
//! - Tuples up to size 5: `impl<S1, S2, S3, ...> Source for (S1, S2, S3, ...)`
//! - Arrays: `impl<S, const N> Source for [S; N]`
//! - and slices: `impl<S> Source for [S]`
//! All the above assume `S: Source`.

pub trait Source<T> {
    fn get(&self) -> Option<T>;

    fn get_existing(&self) -> T;

    fn reset(&self);
}

impl<'a, T, S: Source<T>> Source<T> for &'a S {
    #[inline(always)]
    fn get(&self) -> Option<T> {
        (*self).get()
    }

    #[inline(always)]
    fn get_existing(&self) -> T {
        (*self).get_existing()
    }

    #[inline(always)]
    fn reset(&self) {
        (*self).reset()
    }
}

impl<S1, S2, T1, T2> Source<(T1, T2)> for (S1, S2)
where
    S1: Source<T1>,
    S2: Source<T2>,
{
    #[inline] // Inlined because it is used in the implementation for larger tuple sizes.
    fn get(&self) -> Option<(T1, T2)> {
        match (self.0.get(), self.1.get()) {
            (Some(x1), Some(x2)) => Some((x1, x2)),
            (Some(x1), None) => Some((x1, self.1.get_existing())),
            (None, Some(x2)) => Some((self.0.get_existing(), x2)),
            (None, None) => None,
        }
    }

    #[inline] // Inlined because it is used in the implementation for larger tuple sizes.
    fn get_existing(&self) -> (T1, T2) {
        (self.0.get_existing(), self.1.get_existing())
    }

    #[inline]
    fn reset(&self) {
        self.0.reset();
        self.1.reset();
    }
}

impl<S1, S2, S3, T1, T2, T3> Source<(T1, T2, T3)> for (S1, S2, S3)
where
    S1: Source<T1>,
    S2: Source<T2>,
    S3: Source<T3>,
{
    fn get(&self) -> Option<(T1, T2, T3)> {
        ((&self.0, &self.1), &self.2)
            .get()
            .map(|((x1, x2), x3)| (x1, x2, x3))
    }

    fn get_existing(&self) -> (T1, T2, T3) {
        let ((x1, x2), x3) = ((&self.0, &self.1), &self.2).get_existing();
        (x1, x2, x3)
    }

    fn reset(&self) {
        self.0.reset();
        self.1.reset();
        self.2.reset();
    }
}

impl<S1, S2, S3, S4, T1, T2, T3, T4> Source<(T1, T2, T3, T4)> for (S1, S2, S3, S4)
where
    S1: Source<T1>,
    S2: Source<T2>,
    S3: Source<T3>,
    S4: Source<T4>,
{
    fn get(&self) -> Option<(T1, T2, T3, T4)> {
        ((&self.0, &self.1), (&self.2, &self.3))
            .get()
            .map(|((x1, x2), (x3, x4))| (x1, x2, x3, x4))
    }

    fn get_existing(&self) -> (T1, T2, T3, T4) {
        let ((x1, x2), (x3, x4)) = ((&self.0, &self.1), (&self.2, &self.3)).get_existing();
        (x1, x2, x3, x4)
    }

    fn reset(&self) {
        self.0.reset();
        self.1.reset();
        self.2.reset();
        self.3.reset();
    }
}

impl<S1, S2, S3, S4, S5, T1, T2, T3, T4, T5> Source<(T1, T2, T3, T4, T5)> for (S1, S2, S3, S4, S5)
where
    S1: Source<T1>,
    S2: Source<T2>,
    S3: Source<T3>,
    S4: Source<T4>,
    S5: Source<T5>,
{
    fn get(&self) -> Option<(T1, T2, T3, T4, T5)> {
        (((&self.0, &self.1), &self.2), (&self.3, &self.4))
            .get()
            .map(|(((x1, x2), x3), (x4, x5))| (x1, x2, x3, x4, x5))
    }

    fn get_existing(&self) -> (T1, T2, T3, T4, T5) {
        let (((x1, x2), x3), (x4, x5)) =
            (((&self.0, &self.1), &self.2), (&self.3, &self.4)).get_existing();
        (x1, x2, x3, x4, x5)
    }

    fn reset(&self) {
        self.0.reset();
        self.1.reset();
        self.2.reset();
        self.3.reset();
        self.4.reset();
    }
}

impl<S, T, const N: usize> Source<[T; N]> for [S; N]
where
    S: Source<T>,
{
    fn get(&self) -> Option<[T; N]> {
        let mut found_updated = false;
        let vals_and_sources: [(Option<T>, &S); N] = self.each_ref().map(|s| {
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

    fn reset(&self) {
        for source in self.iter() {
            source.reset();
        }
    }
}

impl<S, T> Source<Vec<T>> for [S]
where
    S: Source<T> + Clone,
{
    fn get(&self) -> Option<Vec<T>> {
        let mut found_updated = false;
        let vals: Vec<Option<T>> = self
            .iter()
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

    fn reset(&self) {
        for source in self.iter() {
            source.reset();
        }
    }
}
