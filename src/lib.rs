use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// A graph of signals and some kind of lazy evaluation.
///
/// This is a graph for signals and derived computations. You should use this graph to create nodes
/// in the graph and to signal a tick in the graph.
pub struct ReactiveGraph {
    /// A value that will be incremented at every reactive step.
    generation: Cell<u64>,
}

impl ReactiveGraph {
    pub fn new() -> Self {
        Self {
            generation: Cell::new(0),
        }
    }
}

#[derive(Debug)]
struct WriteSignalInner<T> {
    /// The most recent value.
    value: T,

    /// A generation that will be incremented every time the value is updated.
    generation: u64,
}

#[derive(Debug)]
pub struct WriteSignal<T>(Rc<RefCell<WriteSignalInner<T>>>);

#[derive(Debug)]
pub struct ReadSignal<T: Clone> {
    source: WriteSignal<T>,

    /// The generation of the value when it was last read.
    generation: Cell<u64>,
}

impl<T> Clone for WriteSignal<T> {
    fn clone(&self) -> Self {
        WriteSignal(self.0.clone())
    }
}

impl<T: Clone> Clone for ReadSignal<T> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            generation: Cell::new(self.generation.get()),
        }
    }
}

impl ReactiveGraph {
    /// Create a new (reader, writer) signal pair.
    // This method need not be a method of `ReactiveGraph` as it makes no use of it, but we keep it
    // like this to align with other signal types.
    pub fn signal<T: Clone>(&self, initial_value: T) -> (ReadSignal<T>, WriteSignal<T>) {
        let write_signal = WriteSignal(Rc::new(RefCell::new(WriteSignalInner {
            value: initial_value,
            generation: 1,
        })));
        let read_signal = ReadSignal {
            source: write_signal.clone(),
            generation: Cell::new(0),
        };
        (read_signal, write_signal)
    }
}

pub trait Source<T> {
    fn get(&self) -> Option<T>;

    fn get_existing(&self) -> T;
}

impl<S1, S2, T1, T2> Source<(T1, T2)> for (S1, S2)
where
    S1: Source<T1>,
    S2: Source<T2>,
{
    #[inline]
    fn get(&self) -> Option<(T1, T2)> {
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
}

impl<S, T> Source<Vec<T>> for [S]
where
    S: Source<T>,
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
}

impl<T: Clone> Source<T> for ReadSignal<T> {
    fn get(&self) -> Option<T> {
        let source = self.source.0.borrow();
        let generation = self.generation.replace(source.generation);
        debug_assert!(generation <= source.generation);
        if generation < source.generation {
            Some(source.value.clone())
        } else {
            None
        }
    }

    fn get_existing(&self) -> T {
        self.source.0.borrow().value.clone()
    }
}

struct DerivedSignalInner<T> {
    rgraph: Rc<ReactiveGraph>,

    /// The function to compute a new value. Returns `None` if non of the sources have been
    /// updated.
    func: Box<dyn FnMut() -> Option<T>>,

    /// The most recently evaluated value.
    last_value: T,

    /// The [`ReactiveGraph::generation`] when this was last evalueated.
    rgraph_generation: u64,
}

#[derive(Clone)]
pub struct DerivedSignal<T: Clone>(Rc<RefCell<DerivedSignalInner<T>>>);

impl ReactiveGraph {
    pub fn derived_signal<S, U, T>(
        self: &Rc<Self>,
        source: S,
        mut f: impl FnMut(U) -> T + 'static,
    ) -> DerivedSignal<T>
    where
        S: Source<U> + 'static,
        T: Clone,
    {
        DerivedSignal(Rc::new(RefCell::new(DerivedSignalInner {
            rgraph: self.clone(),
            last_value: f(source.get_existing()),
            func: Box::new(move || source.get().map(&mut f)),
            rgraph_generation: self.generation.get(),
        })))
    }
}

impl<T: Clone> Source<T> for DerivedSignal<T> {
    fn get(&self) -> Option<T> {
        let mut self_ = self.0.borrow_mut();

        // Check if the derived signal has an older rgraph generation.
        let rgraph_generation = self_.rgraph.generation.get();
        if self_.rgraph_generation < rgraph_generation {
            self_.rgraph_generation = rgraph_generation;

            // Fetch the source.
            if let Some(val) = (self_.func)() {
                self_.last_value = val.clone();
                Some(val)
            } else {
                None
            }
        } else {
            None
        }
    }

    fn get_existing(&self) -> T {
        self.0.borrow().last_value.clone()
    }
}

impl ReactiveGraph {
    pub fn tick(&self) {
        self.generation.set(self.generation.get() + 1);
    }
}
