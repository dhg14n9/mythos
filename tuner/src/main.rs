use mythos::board::board::Board;

pub mod data;

fn main() {
    let mut win = 0;
    let mut draw = 0;
    let mut lose = 0;

    let temp = |_board: &Board, score: f32| {
        if score == 1.0 {
            win += 1;
        }
        else if score == 0.5 {
            draw += 1;
        }
        else {
            lose += 1;
        }
    };

    data::parse_data("tuner/data/quiet-labeled.epd", temp);

    print!("win: {win}\nlose: {lose}\ndraw: {draw}")
    
}
