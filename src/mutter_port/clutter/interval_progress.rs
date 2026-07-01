/// GNOME `clutter/clutter-interval-progress.{c,h}`. Pluggable progress-function
/// registry for custom interpolation (e.g. int, float, color). ClutterProgressFunc
/// signature: type-agnostic closure (const GValue *a, const GValue *b, gdouble
/// progress, GValue *retval) -> gboolean. Rust: use trait object; drop GObject.
use core::any::TypeId;

pub type ProgressFunc = fn(f64) -> f64;

pub struct ProgressRegistry {
    registry: heapless::Vec<(TypeId, ProgressFunc), 256>,
}

impl ProgressRegistry {
    pub const fn new() -> Self {
        ProgressRegistry {
            registry: heapless::Vec::new(),
        }
    }

    pub fn register_progress_func(&mut self, type_id: TypeId, func: ProgressFunc) {
        if let Some(pos) = self.registry.iter().position(|(id, _)| id == &type_id) {
            self.registry[pos].1 = func;
        } else {
            let _ = self.registry.push((type_id, func));
        }
    }

    pub fn lookup_progress_func(&self, type_id: TypeId) -> Option<ProgressFunc> {
        self.registry
            .iter()
            .find(|(id, _)| id == &type_id)
            .map(|(_, func)| *func)
    }

    pub fn run_progress_func(&self, type_id: TypeId, progress: f64) -> ProgressFunc {
        self.lookup_progress_func(type_id)
            .unwrap_or(linear_progress)
    }
}

pub fn linear_progress(progress: f64) -> f64 {
    progress.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_lookup() {
        let mut reg = ProgressRegistry::new();
        let type_id = TypeId::of::<i32>();

        reg.register_progress_func(type_id, |p| p * p);
        let func = reg.lookup_progress_func(type_id).unwrap();
        assert!((func(0.5) - 0.25).abs() < 1e-10);
    }

    #[test]
    fn test_linear_fallback() {
        let reg = ProgressRegistry::new();
        let func = reg.run_progress_func(TypeId::of::<f64>(), 0.5);
        assert!((func(0.5) - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_linear_clamp() {
        assert!((linear_progress(1.5) - 1.0).abs() < 1e-10);
        assert!((linear_progress(-0.5) - 0.0).abs() < 1e-10);
    }
}
