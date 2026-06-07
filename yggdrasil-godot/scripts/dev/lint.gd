extends SceneTree
## Lint headless: carrega TODO .gd em contexto de runtime (autoloads
## registrados) e falha se algum não compilar. Rodado por scripts/godot.sh check.

func _initialize() -> void:
	var files: Array[String] = []
	_walk("res://scripts", files)
	_walk("res://scenes", files)
	var failed := 0
	for f in files:
		if f.ends_with("/dev/lint.gd"):
			continue
		var res: Variant = ResourceLoader.load(f, "Script", ResourceLoader.CACHE_MODE_IGNORE)
		if res == null:
			printerr("LINT-FAIL %s" % f)
			failed += 1
		else:
			print("LINT-OK %s" % f)
	if failed > 0:
		printerr("LINT FAILED: %d scripts" % failed)
		quit(1)
	else:
		print("LINT OK: %d scripts" % files.size())
		quit(0)

func _walk(dir_path: String, out: Array[String]) -> void:
	var d := DirAccess.open(dir_path)
	if d == null:
		return
	d.list_dir_begin()
	var name := d.get_next()
	while name != "":
		var full := dir_path + "/" + name
		if d.current_is_dir():
			_walk(full, out)
		elif name.ends_with(".gd"):
			out.append(full)
		name = d.get_next()
	d.list_dir_end()
