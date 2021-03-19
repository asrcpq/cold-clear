#!/bin/zsh
cd ~/.local/share/tttz/thirdparty/cold-clear/optimizer/best
keys=("clear1" "clear2" "clear4" "back_to_back" "bumpiness\"" "perfect_clear" "\"jeopardy")
for key in $keys; do
	for file in $(ls | grep 'json'); do
		echo ${file%.json} `cat $file | tr ',' '\n' | grep $key | cut -d':' -f2` $key;
	done
done
