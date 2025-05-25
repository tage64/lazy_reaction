use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

/// A graph of signals and some kind of lazy evaluation.
///
/// This is a graph for signals and derived computations. You should use this graph to create nodes
/// in the graph and to signal a tick in the graph.
pub struct ReactiveGraph {
    /// A value that will be incremented at every reactive step.
    generation: AtomicU64,
}

impl ReactiveGraph {
    pub fn new() -> Self {
        Self {
            generation: AtomicU64::new(0),
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
    generation: AtomicU64,
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
            generation: AtomicU64::new(self.generation.load(Ordering::Relaxed)),
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
            generation: AtomicU64::new(0),
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
    #[inline]
    fn get(&self) -> Option<(T1, T2, T3)> {
        ((&self.0, &self.1), &self.2)
            .get()
            .map(|((x1, x2), x3)| (x1, x2, x3))
    }

    #[inline]
    fn get_existing(&self) -> (T1, T2, T3) {
        let ((x1, x2), x3) = ((&self.0, &self.1), &self.2).get_existing();
        (x1, x2, x3)
    }
}

impl<T: Clone> Source<T> for ReadSignal<T> {
    fn get(&self) -> Option<T> {
        let source = self.source.0.borrow();
        let generation = self.generation.swap(source.generation, Ordering::AcqRel);
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
            rgraph_generation: self.generation.load(Ordering::Acquire),
        })))
    }
}

impl<T: Clone> Source<T> for DerivedSignal<T> {
    fn get(&self) -> Option<T> {
        let mut self_ = self.0.borrow_mut();

        // Check if the derived signal has an older rgraph generation.
        let rgraph_generation = self_.rgraph.generation.load(Ordering::Acquire);
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
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}
