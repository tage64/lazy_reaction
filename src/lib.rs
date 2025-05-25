mod source;
pub use source::Source;
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

    /// A call to this method propagates updated values through the graph. Call this when you will
    /// be able to observe the changes of any updated signals since the last tick.
    pub fn tick(&self) {
        self.generation.set(self.generation.get() + 1);
    }
}

#[derive(Debug)]
struct WriteSignalInner<T> {
    /// The most recent value.
    value: RefCell<T>,

    /// A generation that will be incremented every time the value is updated.
    generation: Cell<u64>,
}

/// The writing end of a signal.
///
/// This is a source to the reactive graph. You can write values to this signal, but the change
/// will not be propagated through the reactive graph until the
/// [`tick`-method](ReactiveGraph::tick) has been called. Only the reading end of this signal is
/// able to observe the changes instantly.
#[derive(Debug)]
pub struct WriteSignal<T>(Rc<WriteSignalInner<T>>);

/// The reading end of a signal.
///
/// You typically use this as input to other [derived signals](DerivedSignal), but it also
/// implements the [`Source`] trait, so you could read the value immediately. Beaware that this
/// does not respect the [tick method](ReactiveGraph::tick) of the reactive graph. That is, if you
/// update the value of this signal with [`WriteSignal::set()`], it won't propagate through the
/// graph unless [`ReactiveGraph::tick`] is called, but it will be reflected on this [`ReadSignal`]
/// immediately.
#[derive(Debug, Clone)]
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

/// Create a new (reader, writer) signal pair.
pub fn signal<T: Clone>(initial_value: T) -> (ReadSignal<T>, WriteSignal<T>) {
    // We set the generation of the `WriteSignal` to 1 and of the `ReadSignal` to 0 to make sure
    // the `ReadSignal` returns the first value on the first call to `Source::get()`.
    let write_signal = WriteSignal(Rc::new(WriteSignalInner {
        value: RefCell::new(initial_value),
        generation: Cell::new(1),
    }));
    let read_signal = ReadSignal {
        source: write_signal.clone(),
        generation: Cell::new(0),
    };
    (read_signal, write_signal)
}

impl<T> WriteSignal<T> {
    /// Update the value of the signal, returning the old value.
    pub fn set(&self, value: T) -> T {
        self.0.generation.set(self.0.generation.get() + 1);
        self.0.value.replace(value)
    }
}

impl<T: Clone> Source<T> for ReadSignal<T> {
    fn get(&self) -> Option<T> {
        let source = &self.source.0;

        let source_generation = source.generation.get();
        let generation = self.generation.replace(source_generation);
        debug_assert!(generation <= source_generation);

        if generation < source_generation {
            Some(source.value.borrow().clone())
        } else {
            None
        }
    }

    fn get_existing(&self) -> T {
        self.source.0.value.borrow().clone()
    }
}

struct DerivedSignalInner<T> {
    rgraph: Rc<ReactiveGraph>,

    /// The most recently evaluated value.
    last_value: T,

    /// The function to compute a new value. Takes the reference to the `last_value` and returns
    /// `None` if the value is unchanged.
    func: Box<dyn FnMut(Option<&T>) -> Option<T>>,

    /// The [`ReactiveGraph::generation`] when this was last evaluated.
    rgraph_generation: u64,
}

/// A node in the reactive graph computing its value based on other nodes in the graph.
///
/// Remember that new values will be propagated through the graph only after a call to
/// [`ReactiveGraph::tick()`]. This means that subsequent calls to [`Source::get()`] will return
/// [`None`] till the next call to [`ReactiveGraph::tick()`].
#[derive(Clone)]
pub struct DerivedSignal<T: Clone>(Rc<RefCell<DerivedSignalInner<T>>>);

impl ReactiveGraph {
    /// Create a [`DerivedSignal`]: a node in the reactive graph with an evaluation based on other
    /// sources.
    ///
    /// The source could for instance be a [`ReadSignal`], a [`DerivedSignal`], or a tuple of
    /// sources. The function `f` then excepts the output of this source and computes the new
    /// value.
    ///
    /// Note that calls to [`Source::get()`] on this signal will return [`None`] before the next
    /// call to [`ReactiveGraph::tick()`]. But calls to [`Source::get_existing()`] will return the
    /// existing value of this signal based on the current value of the sources.
    pub fn derived_signal<S, U, T>(
        self: &Rc<Self>,
        source: S,
        mut f: impl FnMut(U) -> T + 'static,
    ) -> DerivedSignal<T>
    where
        S: Source<U> + 'static,
        T: Clone,
    {
        self.derived_signal_with_old_val(source, move |_old_val, x| f(x))
    }

    /// Like [`derived_signal()`](Self::derived_signal) but the evaluation function gets a reference to the previous
    /// value as well (except for the first time).
    pub fn derived_signal_with_old_val<S, U, T>(
        self: &Rc<Self>,
        source: S,
        mut f: impl FnMut(Option<&T>, U) -> T + 'static,
    ) -> DerivedSignal<T>
    where
        S: Source<U> + 'static,
        T: Clone,
    {
        DerivedSignal(Rc::new(RefCell::new(DerivedSignalInner {
            rgraph: self.clone(),
            last_value: f(None, source.get_existing()),
            func: Box::new(move |old_val| source.get().map(|x| f(old_val, x))),
            rgraph_generation: self.generation.get(),
        })))
    }

    /// Create a [`DerivedSignal`] that only propagates a new value if it has changed according
    /// to the [`PartialEq`] implementation.
    pub fn memo<S, U, T>(
        self: &Rc<Self>,
        source: S,
        mut f: impl FnMut(U) -> T + 'static,
    ) -> DerivedSignal<T>
    where
        S: Source<U> + 'static,
        T: Clone + PartialEq,
    {
        self.memo_with_comparator(source, move |_old_val, x| f(x), |lhs, rhs| lhs == rhs)
    }

    /// Like [`memo()`](Self::memo) but the evaluation function takes a mutable reference to the previous value
    /// as well (except for the first evaluation).
    pub fn memo_with_old_val<S, U, T>(
        self: &Rc<Self>,
        source: S,
        mut f: impl FnMut(Option<&T>, U) -> T + 'static,
    ) -> DerivedSignal<T>
    where
        S: Source<U> + 'static,
        T: Clone + PartialEq,
    {
        self.memo_with_comparator(
            source,
            move |old_val, x| f(old_val, x),
            |lhs, rhs| lhs == rhs,
        )
    }

    /// Create a [`DerivedSignal`] that only propagate a new value if it has changed according
    /// to a comparison function.
    pub fn memo_with_comparator<S, U, T>(
        self: &Rc<Self>,
        source: S,
        mut f: impl FnMut(Option<&T>, U) -> T + 'static,
        mut is_equal: impl FnMut(&T, &T) -> bool + 'static,
    ) -> DerivedSignal<T>
    where
        S: Source<U> + 'static,
        T: Clone,
    {
        DerivedSignal(Rc::new(RefCell::new(DerivedSignalInner {
            rgraph: self.clone(),
            last_value: f(None, source.get_existing()),
            func: Box::new(move |old_val| {
                source
                    .get()
                    .map(|x| f(old_val, x))
                    .filter(|val| !is_equal(old_val.expect("last_value should exist"), &val))
            }),
            rgraph_generation: self.generation.get(),
        })))
    }
}

impl<T: Clone> Source<T> for DerivedSignal<T> {
    fn get(&self) -> Option<T> {
        let mut self_ = self.0.borrow_mut();
        let self_ = &mut *self_; // To be able to borrow multiple fields simultaneously.

        // Check if the derived signal has an older rgraph generation.
        let rgraph_generation = self_.rgraph.generation.get();
        if self_.rgraph_generation < rgraph_generation {
            self_.rgraph_generation = rgraph_generation;

            // Fetch the source.
            if let Some(val) = (self_.func)(Some(&self_.last_value)) {
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
