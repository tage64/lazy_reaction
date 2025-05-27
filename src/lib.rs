//! An implementation of a semi-reactive system, similar to
//! [reactive_graph](https://docs.rs/reactive_graph) but much simpler. In particular, unlike
//! reactive_graph, this implementation does not require a global runtime. Instead, updates become
//! visible in [derived signals](DerivedSignal) only after a call to [`ReactiveGraph::tick()`].
//!
//! However, the most notable difference from reactive_graph is that this implementation does not
//! include any side effects such as the `Effect` type. You must manually query nodes in the graph
//! after calling [`tick()`](ReactiveGraph::tick). Consequently, the entire graph does not need to
//! be reevaluated at every tick; only the relevant parts of the graph are updated when you call
//! [`Source::get()`].
//!
//! In short, the system operates as follows:
//!
//! - You create a set of signals using the [`signal()`] function. These act as sources for the
//!   reactive graph and receive values from the outside world. Each signal comes in a pair:
//!   ([`ReadSignal`], [`WriteSignal`]). The [`ReadSignal`] type implements the [`Source`] trait,
//!   which provides the following methods:
//!
//!     - [`Source::get()`]: Returns `Some(val)` if the signal has been updated since the last call
//!       to this method, or [`None`] otherwise. For [`ReadSignal`], it returns [`Some`] on the
//!       first call with the initial value; this is not the case for [`DerivedSignal`] (see below).
//!
//!     - [`Source::get_existing()`]: Returns the current value. This requires that values
//!       implement [`Clone`]. If you are working with large objects, consider wrapping them in a
//!       shared pointer such as [`Rc`].
//!
//! - You create a graph for all derived signals and memos with [`ReactiveGraph::new()`].
//!
//! - You add [derived signals](DerivedSignal) using [`ReactiveGraph::derived_signal()`] or
//!   [`ReactiveGraph::memo()`]. These are nodes that depend on one or more sources within the
//!   reactive graph. The sources may be [pure signals](ReadSignal) or other [derived signals]
//!   (`DerivedSignal`). Any type implementing the [`Source`] trait can serve as a source.
//!
//!     [`DerivedSignal`] also implements the [`Source`] trait, with one notable distinction from
//!     [`ReadSignal`]: the [`get()`-function](Source::get) may return a new value (`Some`) only
//!     after a call to [`ReactiveGraph::tick()`]. Subsequent calls to [`Source::get()`] will return
//!     [`None`] until the next tick. This also applies after its creation—`get()` will return
//!     [`None`] until the first tick.
//!
//! - You update the system by pushing new values to signals using [`WriteSignal::set()`].
//!
//! - When you want to observe changes, you call [`ReactiveGraph::tick()`].
//!
//! - You use [`DerivedSignal::get()`] to retrieve the value of a derived signal. If it returns
//!   [`None`], the value has not changed since you (or another derived signal) last called `get()`.
//!   You can always retrieve the latest value (as of the last tick) with
//!   [`DerivedSignal::get_existing()`], regardless of whether it has changed.
//!
//! So, why is it lazy? Because derived values are only computed on demand. In reactive_graph,
//! derived signals are reevaluated on every tick of the runtime, and relevant side effects are
//! triggered. Here, you decide when and which parts of the graph should be reevaluated. Only when
//! you call [`DerivedSignal::get()`] are the necessary parts of the graph recomputed.
//!
//! ### Example
//!
//! ```
//! use lazy_reaction::{ReactiveGraph, Source, signal};
//!
//! let (get_year, set_year) = signal(1989);
//! let (get_month, set_month) = signal(11);
//! let (get_day, set_day) = signal(9);
//!
//! // Initial call to get() will return the values but subsequent will not.
//! assert_eq!(get_year.get(), Some(1989));
//! assert_eq!(get_year.get(), None);
//! assert_eq!(get_year.get(), None);
//! // However get_existing() works.
//! assert_eq!(get_year.get_existing(), 1989);
//! assert_eq!(get_month.get_existing(), 11);
//!
//! // Let's create derived signals that format these items.
//! // We begin by constructing a reactive graph:
//! let rgraph = ReactiveGraph::new();
//!
//! // And then we add the derived signals.
//! // get_year is the source, and is cheaply clonable.
//! let formatted_year = rgraph.derived_signal(get_year.clone(), |year| year.to_string());
//!
//! assert_eq!(formatted_year.get(), Some("1989".to_string()));
//! assert_eq!(formatted_year.get(), None);
//! assert_eq!(formatted_year.get_existing(), "1989");
//!
//! // It is usually better to use a memo. Then the value will only be considered updated if it has
//! // changed.
//! let formatted_month = rgraph.memo(get_month, |m| format!("{m:02}"));
//! let formatted_day = rgraph.memo(get_day, |d| format!("{d:02}"));
//!
//! // Now we show the real power of the system! We agrigate these derived signals to a formatted
//! // date. This will only be reevaluated if any of the input signals changed.
//! let formatted_date = rgraph.memo(
//!     (formatted_year, formatted_month, formatted_day),
//!     |(yyyy, mm, dd)| format!("{yyyy}-{mm}-{dd}"),
//! );
//!
//! // A value is immediately available based on the get_existing() calls of the sources.
//! assert_eq!(formatted_date.get(), Some("1989-11-09".to_string()));
//! assert_eq!(formatted_date.get(), None);
//!
//! // Let's make a tick of the graph!
//! rgraph.tick();
//!
//! // Nothing has changed so get() still returns None.
//! assert_eq!(formatted_date.get(), None);
//! assert_eq!(formatted_date.get_existing(), "1989-11-09");
//!
//! // Let's change dates.
//! set_year.set(1991);
//! assert_eq!(get_year.get(), Some(1991));
//! set_month.set(12);
//! set_day.set(8);
//! // We haven't called tick() yet so nothing has been updated.
//! assert_eq!(formatted_date.get(), None);
//!
//! rgraph.tick();
//! assert_eq!(formatted_date.get(), Some("1991-12-08".to_string()));
//!
//! set_year.set(2024);
//!
//! rgraph.tick();
//! assert_eq!(formatted_date.get(), Some("2024-12-08".to_string()));
//! ```

mod source;
use std::cell::{Cell, RefCell};
use std::num::NonZero;
use std::rc::Rc;

pub use source::Source;

/// A graph of signals and some kind of lazy evaluation.
///
/// This is a graph for signals and derived computations. You should use this graph to create nodes
/// in the graph and to signal a tick in the graph.
pub struct ReactiveGraph {
    /// A value that will be incremented at every reactive step.
    generation: Cell<NonZero<u64>>,
}

impl Default for ReactiveGraph {
    fn default() -> Self {
        Self {
            generation: Cell::new(NonZero::new(1).unwrap()),
        }
    }
}

impl ReactiveGraph {
    pub fn new() -> Rc<Self> {
        Rc::new(Self::default())
    }

    /// A call to this method propagates updated values through the graph. Call this when you will
    /// be able to observe the changes of any updated signals since the last tick.
    pub fn tick(&self) {
        self.generation.set(
            self.generation
                .get()
                .checked_add(1)
                .expect("ReactiveGraph generation overflow"),
        );
    }
}

#[derive(Debug)]
struct WriteSignalInner<T> {
    /// The most recent value.
    value: RefCell<T>,

    /// A generation that will be incremented every time the value is updated.
    generation: Cell<NonZero<u64>>,
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
        generation: Cell::new(NonZero::new(1).unwrap()),
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
        self.0.generation.set(
            self.0
                .generation
                .get()
                .checked_add(1)
                .expect("WriteSignal generation overflow"),
        );
        self.0.value.replace(value)
    }
}

impl<T: Clone> Source<T> for ReadSignal<T> {
    fn get(&self) -> Option<T> {
        let source = &self.source.0;

        let source_generation = source.generation.get().get();
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

    fn reset(&self) {
        self.generation.set(0);
    }
}

struct NodeInner<T> {
    /// The most recently evaluated value.
    value: T,

    /// The function to compute a new value. Takes a reference to the `last_value` and returns
    /// `None` if the value is unchanged.
    func: Box<dyn FnMut(&T) -> Option<T>>,
}

struct Node<T> {
    // This is stored in an inner-struct so it can be wrapped in a `RefCell`.
    inner: RefCell<NodeInner<T>>,

    /// The [`ReactiveGraph`] for this node.
    graph: Rc<ReactiveGraph>,

    /// The [`ReactiveGraph::generation`] when this was last evaluated, or 0 if it never has been
    /// generated.
    graph_generation: Cell<u64>,

    /// The generation of this value.
    val_generation: Cell<NonZero<u64>>,
}

/// A node in the reactive graph computing its value based on other nodes in the graph.
///
/// Remember that new values will be propagated through the graph only after a call to
/// [`ReactiveGraph::tick()`]. This means that subsequent calls to [`Source::get()`] will return
/// [`None`] till the next call to [`ReactiveGraph::tick()`].
#[derive(Clone)]
pub struct DerivedSignal<T: Clone> {
    node: Rc<Node<T>>,

    /// The value generation when this reader last called [`Source::get()`], or 0 if it has never
    /// been called or if reset.
    val_generation: Cell<u64>,
}

impl<T: Clone> DerivedSignal<T> {
    /// Convenience method to avoid code duplication.
    fn new<S, U>(
        graph: Rc<ReactiveGraph>,
        source: S,
        mut f: impl FnMut(Option<&T>, U) -> T + 'static,
        mut has_changed: impl FnMut(&T, &T) -> bool + 'static,
    ) -> Self
    where
        S: Source<U> + 'static,
    {
        source.reset();

        let value = f(None, source.get_existing());
        let func = Box::new(move |prev_val: &T| {
            source
                .get()
                .map(|x| f(Some(prev_val), x))
                .filter(|x| has_changed(prev_val, x))
        });
        Self {
            node: Rc::new(Node {
                inner: RefCell::new(NodeInner { value, func }),
                graph,
                graph_generation: Cell::new(0),
                val_generation: Cell::new(NonZero::new(1).unwrap()),
            }),
            val_generation: Cell::new(0),
        }
    }
}

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
        f: impl FnMut(Option<&T>, U) -> T + 'static,
    ) -> DerivedSignal<T>
    where
        S: Source<U> + 'static,
        T: Clone,
    {
        DerivedSignal::new(self.clone(), source, f, |_, _| true)
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
        self.memo_with_comparator(source, move |_old_val, x| f(x), |lhs, rhs| lhs != rhs)
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
            |lhs, rhs| lhs != rhs,
        )
    }

    /// Create a [`DerivedSignal`] that only propagate a new value if it has changed according
    /// to a comparison function.
    pub fn memo_with_comparator<S, U, T>(
        self: &Rc<Self>,
        source: S,
        f: impl FnMut(Option<&T>, U) -> T + 'static,
        has_changed: impl FnMut(&T, &T) -> bool + 'static,
    ) -> DerivedSignal<T>
    where
        S: Source<U> + 'static,
        T: Clone,
    {
        DerivedSignal::new(self.clone(), source, f, has_changed)
    }
}

impl<T: Clone> Source<T> for DerivedSignal<T> {
    fn get(&self) -> Option<T> {
        // Check if a tick has occurred on the graph and we need to compute a new value.
        let graph_generation = self.node.graph.generation.get().get();
        let my_graph_generation = self.node.graph_generation.replace(graph_generation);
        debug_assert!(my_graph_generation <= graph_generation);
        if my_graph_generation < graph_generation {
            // Check if the source has a newer value.
            let mut inner = self.node.inner.borrow_mut();
            let inner = &mut *inner; // To be able to borrow fields simultaneously.
            if let Some(x) = (inner.func)(&inner.value) {
                // A new value is computed!
                inner.value = x.clone();
                // Bump the value generation.
                self.node.val_generation.set(
                    self.node
                        .val_generation
                        .get()
                        .checked_add(1)
                        .expect("DerivedSignal: val_generation overflow"),
                );
                self.val_generation
                    .set(self.node.val_generation.get().get());
                return Some(x);
            }
        }

        // Check if self.val_generation is lagging behind self.node.val_generation.
        let val_generation = self.node.val_generation.get().get();
        let my_val_generation = self.val_generation.replace(val_generation);
        debug_assert!(my_val_generation <= val_generation);
        if my_val_generation < val_generation {
            return Some(self.get_existing());
        }

        None
    }

    fn get_existing(&self) -> T {
        self.node.inner.borrow().value.clone()
    }

    fn reset(&self) {
        self.val_generation.set(0);
    }
}
