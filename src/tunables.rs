#[cfg(feature = "tunables")]
use std::sync::atomic::{AtomicI32, Ordering};

#[cfg(feature = "tunables")]
const R_END: f64 = 0.002;

#[cfg(feature = "tunables")]
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
            #[cfg(feature = "tunables")]
            mod $name {
                pub static VALUE: std::sync::atomic::AtomicI32 =
                    std::sync::atomic::AtomicI32::new($default);
            }

            #[cfg(feature = "tunables")]
            #[inline(always)]
            pub fn $name() -> i32 {
                $name::VALUE.load(std::sync::atomic::Ordering::Relaxed)
            }

            #[cfg(not(feature = "tunables"))]
            #[inline(always)]
            pub const fn $name() -> i32 { $default }
        )*

        #[cfg(feature = "tunables")]
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
    // reverse futility pruning
    rfp_margin_mult       =      100,     40,    250,   10.0;
    rfp_max_depth         =        6,      3,     10,    0.5;

    // razoring
    razor_margin          =      150,    150,   1000,   45.0;
    razor_max_depth       =        3,      2,      8,    0.5;

    // null move pruning
    nmp_min_depth         =        5,      2,      8,    0.5;
    nmp_base              =        4,      2,      5,    0.5;
    nmp_depth_div         =        3,      2,      6,    0.5;

    // late move reductions
    // lmr_base and lmr_div are scaled by 100
    lmr_min_moves         =        2,      2,      6,    0.5;
    lmr_min_depth         =        3,      2,      5,    0.5;
    lmr_base              =      128,      0,    200,   10.0;
    lmr_div               =      244,    100,    400,   15.0;
    lmr_hist_div          =     6920,   2000,  20000,  900.0;
    lmr_cut_node          =        2,      0,      3,    0.5;

    // late move pruning
    lmp_base              =        3,      1,      8,    0.5;
    lmp_max_depth         =        8,      4,     12,    0.5;

    // futility pruning
    fp_margin_mult        =      125,     40,    250,   10.0;
    fp_max_depth          =        9,      3,     10,    0.5;

    // SEE pruning
    see_quiet_margin      =      -65,   -150,    -10,    7.0;
    see_noisy_margin      =     -141,   -200,    -20,    9.0;

    // history pruning
    hist_prune_margin     =     1604,    200,   3000,  140.0;

    // internal iterative reduction
    iir_min_depth         =        3,      2,      8,    0.5;

    // aspiration windows
    asp_window            =       26,      8,     60,    2.5;

    // history bonus/malus
    hist_quiet_bonus_mult  =     189,     60,    400,   17.0;
    hist_quiet_bonus_max   =    1624,    600,   3000,  120.0;
    hist_quiet_bonus_off   =      72,      0,    300,   15.0;

    hist_quiet_malus_mult  =     201,     60,    400,   17.0;
    hist_quiet_malus_max   =     719,    400,   2400,  100.0;
    hist_quiet_malus_off   =      20,      0,    300,   15.0;
    hist_quiet_malus_decay =      12,      0,    100,    5.0;

    hist_cont_bonus_mult   =     201,     40,    300,   13.0;
    hist_cont_bonus_max    =    1852,    400,   2400,  100.0;
    hist_cont_bonus_off    =      47,      0,    300,   15.0;

    hist_cont_malus_mult   =     351,    150,    900,   38.0;
    hist_cont_malus_max    =     963,    300,   2000,   85.0;
    hist_cont_malus_off    =      22,      0,    300,   15.0;
    hist_cont_malus_decay  =      22,      0,    100,    5.0;

    // capture history
    hist_noisy_bonus_mult  =     127,     60,    400,   17.0;
    hist_noisy_bonus_max   =    1544,    600,   3000,  120.0;
    hist_noisy_bonus_off   =      77,      0,    300,   15.0;

    hist_noisy_malus_mult  =     223,     60,    400,   17.0;
    hist_noisy_malus_max   =     689,    400,   2400,  100.0;
    hist_noisy_malus_off   =       0,      0,    300,   15.0;
    hist_noisy_malus_decay =      11,      0,    100,    5.0;

    noisy_mvv_mult         =      24,      4,     64,    3.0;
    hist_malus_scale_decay =      41,      0,    150,    7.0;

    // time management
    tm_stab_base           =     308,    150,    500,   17.0;
    tm_stab_step           =      52,      0,     80,    4.0;
    tm_stab_min            =     112,     40,    150,    5.5;
    tm_stab_min_depth      =       9,      4,     14,    0.5;
    tm_node_base           =     137,    100,    250,    7.0;
    tm_node_scale          =     140,     50,    300,   12.0;
    // Eval trend: zero-sum weights over the last 6 scores (oldest first), a5 = -(a0+..+a4)
    tm_trend_a0            =     -62,   -150,    150,   10.0;
    tm_trend_a1            =     -83,   -150,    150,   10.0;
    tm_trend_a2            =     -31,   -150,    150,   10.0;
    tm_trend_a3            =       4,   -150,    150,   10.0;
    tm_trend_a4            =      38,   -150,    150,   10.0;
    tm_trend_lo            =      72,     30,    100,    4.0;
    tm_trend_hi            =     166,    100,    250,    6.0;

    // singular / double extension
    se_margin              =      29,      8,    100,    4.0;
    se_double_margin       =      11,      4,    120,    5.0;
    se_cut_node_neg_ext    =       3,      0,      3,    0.5;

    // improving (rfp only for now)
    improving_threshold    =       4,      0,    240,   12.0;
    rfp_improvement_mult   =      43,      0,    200,   10.0;

    corr_bonus_mult        =     209,     50,    600,   25.0;
    corr_div               =      95,     16,    256,   12.0;
    corr_w_pawn            =      85,      0,    256,   12.0;
    corr_w_non_pawn        =     115,      0,    256,   12.0; // both color shared
    corr_w_minor           =      48,      0,    256,   12.0;
    // corr_w_major           =      32,      0,    256,   12.0;

    // probcut
    probcut_margin        =      200,    100,    400,   15.0;
    probcut_improving     =       50,      0,    150,    8.0;
    probcut_min_depth     =        5,      3,      8,    0.5;
    probcut_reduction     =        4,      3,      5,    0.5;
    small_probcut_margin  =      400,    200,    700,   25.0;

}

#[cfg(feature = "tunables")]
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

#[cfg(feature = "tunables")]
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


#[cfg(feature = "tunables")]
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

#[cfg(not(feature = "tunables"))]
pub fn print_options() {}

#[cfg(not(feature = "tunables"))]
pub fn set(_name: &str, _value: &str) -> bool { false }

#[cfg(not(feature = "tunables"))]
pub fn print_spsa() {
    println!("info string built without the tunables feature; rebuild with --features tunables");
}
