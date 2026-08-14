use std::sync::atomic::{AtomicI32, Ordering};

const R_END: f64 = 0.002;

pub struct Spec {
    pub name: &'static str,
    pub value: &'static AtomicI32,
    pub min: i32,
    pub max: i32,
    pub c_end: f64,
}

macro_rules! tunables {
    ($($name:ident = $default:expr, $min:expr, $max:expr, $c_end:expr;)*) => {
        $(
            mod $name {
                pub static VALUE: std::sync::atomic::AtomicI32 =
                    std::sync::atomic::AtomicI32::new($default);
            }

            #[inline(always)]
            pub fn $name() -> i32 {
                $name::VALUE.load(std::sync::atomic::Ordering::Relaxed)
            }
        )*

        pub const SPECS: &[Spec] = &[
            $(Spec {
                name: stringify!($name),
                value: &$name::VALUE,
                min: $min,
                max: $max,
                c_end: $c_end,
            }),*
        ];
    };
}

//        name                default   min     max    c_end
tunables! {
    // Reverse futility pruning.
    rfp_margin_mult       =      150,     40,    250,   10.0;
    rfp_max_depth         =        5,      3,     10,    0.5;

    // Null move pruning
    nmp_min_depth         =        3,      2,      5,    0.5;
    nmp_base              =        3,      2,      5,    0.5;
    nmp_depth_div         =        3,      2,      6,    0.5;

    // Late move reductions. lmr_base and lmr_div are scaled by 100.
    lmr_min_moves         =        4,      2,      6,    0.5;
    lmr_min_depth         =        3,      2,      5,    0.5;
    lmr_base              =       75,      0,    200,   10.0;
    lmr_div               =      225,    100,    400,   15.0;
    lmr_hist_div          =    10000,   2000,  20000,  900.0;

    // Late move pruning
    lmp_base              =        3,      1,      8,    0.5;
    lmp_max_depth         =        8,      4,     12,    0.5;

    // Futility pruning, margin per ply of depth
    fp_margin_mult        =      100,     40,    250,   10.0;
    fp_max_depth          =        6,      3,     10,    0.5;

    // SEE pruning thresholds, per ply of depth
    see_quiet_margin      =      -50,   -150,    -10,    7.0;
    see_noisy_margin      =     -100,   -200,    -20,    9.0;

    // History pruning
    hist_prune_margin     =     1000,    200,   3000,  140.0;

    // Aspiration windows
    asp_window            =       30,      8,     60,    2.5;
}

pub fn print_options() {
    for spec in SPECS {
        println!(
            "option name {} type spin default {} min {} max {}",
            spec.name,
            spec.value.load(Ordering::Relaxed),
            spec.min,
            spec.max
        );
    }
}

pub fn set(name: &str, value: &str) -> bool {
    let Some(spec) = SPECS.iter().find(|s| s.name.eq_ignore_ascii_case(name)) else {
        return false;
    };

    match value.parse::<i32>() {
        Ok(v) => spec.value.store(v.clamp(spec.min, spec.max), Ordering::Relaxed),
        Err(_) => println!("info string invalid value for {}", spec.name),
    }
    true
}


pub fn print_spsa() {
    for spec in SPECS {
        println!(
            "{}, int, {}, {}, {}, {}, {}",
            spec.name,
            spec.value.load(Ordering::Relaxed),
            spec.min,
            spec.max,
            spec.c_end,
            R_END
        );
    }
}
