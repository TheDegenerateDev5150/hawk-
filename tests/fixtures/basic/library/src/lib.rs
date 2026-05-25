pub fn entry() {
    internal_helper();
}

pub fn internal_helper() {}

pub struct ProductValue;

pub fn product_value() -> ProductValue {
    ProductValue
}

pub struct InternalNamespace;

impl InternalNamespace {
    pub fn live_inside_crate() {}

    pub fn dead_method() {}
}

pub fn use_namespace() {
    InternalNamespace::live_inside_crate();
}

pub fn dead_entry() {
    dead_helper();
}

pub fn dead_helper() {}

#[allow(dead_code)]
pub fn retained_entry() {
    retained_helper();
}

pub fn retained_helper() {}
