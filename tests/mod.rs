use lazy_reaction::{ReactiveGraph, Source, signal};

#[test]
fn test_signals() {
    let (mut get_year, set_year) = signal(1789);
    assert_eq!(get_year.get_existing(), 1789);
    assert_eq!(get_year.get(), Some(1789));
    assert_eq!(get_year.get(), None);
    assert_eq!(get_year.get(), None);
    assert_eq!(get_year.get_existing(), 1789);

    // Test reset.
    get_year.reset();
    assert_eq!(get_year.get(), Some(1789));
    assert_eq!(get_year.get(), None);

    // Update the value.
    assert_eq!(set_year.set(1797), 1789);
    assert_eq!(
        get_year.get_existing(),
        1797,
        "the value is updated immediately in signals"
    );
    assert_eq!(get_year.get(), Some(1797));
    assert_eq!(get_year.get_existing(), 1797);
    assert_eq!(get_year.get(), None);
    get_year.reset();
    assert_eq!(get_year.get_existing(), 1797);
    assert_eq!(get_year.get(), Some(1797));

    // Push two values with no read in between.
    set_year.set(1803);
    set_year.set(1809);
    assert_eq!(get_year.get(), Some(1809));
    assert_eq!(get_year.get_existing(), 1809);
    assert_eq!(get_year.get(), None);
    assert_eq!(get_year.get(), None);
    assert_eq!(get_year.get_existing(), 1809);
}

#[test]
fn test_derived_signals() {
    let rgraph = ReactiveGraph::new();

    let (get_year, set_year) = signal(1813);
    let (get_name, _) = signal("Elizabeth");
    let mut text = rgraph.derived_signal((get_year, get_name), |(year, name)| {
        format!("{year}: {name}")
    });

    assert_eq!(text.get_existing(), "1813: Elizabeth");
    assert_eq!(text.get(), Some("1813: Elizabeth".to_string()));
    assert_eq!(text.get(), None);
    assert_eq!(text.get(), None);
    assert_eq!(text.get_existing(), "1813: Elizabeth");

    set_year.set(1817);
    assert_eq!(text.get_existing(), "1813: Elizabeth");
    assert_eq!(text.get(), None);

    // We only observe the changes after a tick.
    rgraph.tick();
    assert_eq!(text.get_existing(), "1813: Elizabeth");
    assert_eq!(text.get(), Some("1817: Elizabeth".to_string()));
    assert_eq!(text.get(), None);
    assert_eq!(text.get_existing(), "1817: Elizabeth");

    // A new tick doesn't change anything.
    rgraph.tick();
    assert_eq!(text.get(), None);
    assert_eq!(text.get_existing(), "1817: Elizabeth");

    // However we may reset.
    text.reset();
    assert_eq!(text.get_existing(), "1817: Elizabeth");
    assert_eq!(text.get(), Some("1817: Elizabeth".to_string()));
    assert_eq!(text.get(), None);
}
