fn main() {
    library::entry();
    let _ = library::product_value();
    let _ = library::product_context();
    library::exercise_private_context();
    library::exercise_reexported_value();
    library::use_namespace();
    library::exercise_constructors();
    library::through_reexport();
}
