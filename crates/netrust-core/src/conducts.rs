use netrust_types::ConductTracker;

pub fn record_kill(tracker: &mut ConductTracker) {
    tracker.pacifist = false;
}

pub fn record_eat_meat(tracker: &mut ConductTracker) {
    tracker.vegan = false;
    tracker.vegetarian = false;
}

pub fn record_read(tracker: &mut ConductTracker) {
    tracker.illiterate = false;
}

pub fn record_altar_action(tracker: &mut ConductTracker) {
    tracker.atheist = false;
}

pub fn record_wish(tracker: &mut ConductTracker) {
    tracker.wishless = false;
}

pub fn record_polypile(tracker: &mut ConductTracker) {
    tracker.polypileless = false;
}
