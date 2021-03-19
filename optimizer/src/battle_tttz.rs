use tttz_mpboard::Game;
use tttz_ai::CCBot;
use tttz_ai::Thinker;
use cold_clear::evaluation::Standard;

pub fn do_battle(
    p1: Standard, p2: Standard,
) -> Option<((), bool)> {
	let mut game = Game::new(1, 2, [].iter());
	let mut bots = [
		CCBot::from_eval(p1),
		CCBot::from_eval(p2),
	];
	let mut player = 0; // current player index
	loop {
		let display = game.generate_display(player, 0);
		let ret = bots[player].main_think(display);
		if ret.is_empty() {
			return Some(((), player == 2))
		}
		for key_type in ret.into_iter() {
			let ret = game.process_key(player as i32 + 1, 0, key_type).0;
			if ret > 0 {
				return Some(((), ret == 1))
			}
		}
		player = 1 - player;
	}
}
