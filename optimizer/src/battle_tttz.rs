use tttz_mpboard::Game;
use tttz_ai::CCBot;
use tttz_ai::Thinker;
use cold_clear::Interface;
use cold_clear::evaluation::Standard;

pub fn do_battle(
    p1: Standard, p2: Standard,
) -> Option<((), bool)> {
	let mut game = Game::new(1, 2, [].iter());
	let mut bots = [
		CCBot {
			interface: Interface::launch(
				libtetris::Board::new(),
				Default::default(),
				p1,
				None,
			),
			preview_list: [7; 6],
		},
		CCBot {
			interface: Interface::launch(
				libtetris::Board::new(),
				Default::default(),
				p2,
				None,
			),
			preview_list: [7; 6],
		},
	];
	let mut player = 0; // current player index
	loop {
		let display = game.generate_display(player, 0);
		for key_type in bots[player].main_think(display).into_iter() {
			let ret = game.process_key(player as i32 + 1, 0, key_type).0;
			if ret > 0 {
				return Some(((), ret == 1))
			}
		}
		player = 1 - player;
	}
}
