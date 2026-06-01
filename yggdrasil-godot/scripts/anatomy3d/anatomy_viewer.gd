extends Node3D
## Visualizador 3D interativo de anatomia (estilo TeachMeAnatomy) — YG-83/YG-87.
##
## Câmera orbital sobre um corpo translúcido com o SNC (encéfalo + medula) por
## dentro. Navegação SEM arrastar: controles na tela (zoom/girar/inclinar) +
## atalhos de teclado; clicar numa estrutura CENTRALIZA nela e mostra o nome.
## Um slider controla a transparência do corpo. Malhas reais do BodyParts3D
## (CC-BY-SA 2.1 JP / DBCLS) — ver assets/anatomia/ATTRIBUTION.md.

# id → (arquivo, rótulo PT, cor, alpha, é_corpo)
const PARTS := {
	"body_skin": {"path": "res://assets/anatomia/body_skin.obj", "label": "Pele (corpo)", "color": Color(0.93, 0.79, 0.72), "alpha": 0.16, "body": true},
	"brain": {"path": "res://assets/anatomia/brain.obj", "label": "Encéfalo", "color": Color(0.96, 0.85, 0.86), "alpha": 1.0, "body": false},
	"spinal_cord": {"path": "res://assets/anatomia/spinal_cord.obj", "label": "Medula espinhal", "color": Color(1.0, 0.9, 0.36), "alpha": 1.0, "body": false},
}

const ROT_SPEED := 1.3        # rad/s ao girar/inclinar (botão ou tecla)
const ZOOM_RATE := 0.9        # fator/s ao segurar zoom
const WHEEL_STEP := 0.96      # zoom por "notch" de scroll (suave; <1 = aproxima)

var cam: Camera3D
var model: Node3D
var body_mat: StandardMaterial3D
var label_pick: Label
var instances: Dictionary = {}

var target := Vector3.ZERO
var home_target := Vector3.ZERO
var yaw := 0.7
var pitch := 0.15
var distance := 5.0
var home_distance := 5.0
var near_d := 0.05
var far_d := 100.0

# direção mantida pelos botões da tela (-1/0/+1 por eixo)
var hold := {"yaw": 0.0, "pitch": 0.0, "zoom": 0.0}


func _ready() -> void:
	_setup_environment()

	model = Node3D.new()
	model.name = "Model"
	model.rotation_degrees = Vector3(-90, 0, 0)  # BodyParts3D Z-up → Y-up
	add_child(model)

	var aabb := AABB()
	var first := true
	for id in PARTS:
		var mesh: Mesh = load(PARTS[id]["path"])
		if mesh == null:
			push_warning("malha ausente: %s" % PARTS[id]["path"])
			continue
		var mi := MeshInstance3D.new()
		mi.name = id
		mi.mesh = mesh
		var mat := StandardMaterial3D.new()
		mat.albedo_color = Color(PARTS[id]["color"], PARTS[id]["alpha"])
		mat.roughness = 0.6
		if PARTS[id]["alpha"] < 1.0:
			mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
			mat.cull_mode = BaseMaterial3D.CULL_DISABLED
			body_mat = mat
		else:
			mi.create_trimesh_collision()  # estruturas internas → click-to-center
			for c in mi.get_children():
				if c is StaticBody3D:
					c.set_meta("part_label", PARTS[id]["label"])
		mi.material_override = mat
		model.add_child(mi)
		instances[id] = mi
		var mab: AABB = model.transform * mesh.get_aabb()
		aabb = mab if first else aabb.merge(mab)
		first = false

	target = aabb.get_center()
	home_target = target
	distance = maxf(aabb.size.length() * 0.85, 1.0)
	home_distance = distance
	near_d = maxf(distance * 0.005, 0.02)
	far_d = distance * 12.0

	cam = Camera3D.new()
	cam.near = near_d
	cam.far = far_d
	add_child(cam)
	cam.current = true
	_update_cam()

	_build_ui()

	if OS.get_environment("CAPTURE") == "1":
		await _capture_sequence()


func _setup_environment() -> void:
	var we := WorldEnvironment.new()
	var env := Environment.new()
	env.background_mode = Environment.BG_COLOR
	env.background_color = Color(0.04, 0.04, 0.06)
	env.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	env.ambient_light_color = Color(0.55, 0.57, 0.62)
	env.ambient_light_energy = 0.7
	we.environment = env
	add_child(we)
	var key := DirectionalLight3D.new()
	key.rotation_degrees = Vector3(-55, -35, 0)
	key.light_energy = 1.1
	add_child(key)
	var fill := DirectionalLight3D.new()
	fill.rotation_degrees = Vector3(-15, 135, 0)
	fill.light_energy = 0.4
	add_child(fill)


func _update_cam() -> void:
	var dir := Vector3(cos(pitch) * sin(yaw), sin(pitch), cos(pitch) * cos(yaw))
	cam.global_position = target + dir * distance
	cam.look_at(target, Vector3.UP)


func _zoom_by(factor: float) -> void:
	distance = clampf(distance * factor, near_d * 2.0, far_d * 0.5)
	_update_cam()


# ---- Navegação contínua: botões da tela + teclado (sem arrastar) ----

func _process(delta: float) -> void:
	var dy: float = hold.yaw
	var dp: float = hold.pitch
	var dz: float = hold.zoom
	# atalhos de teclado (setas / WASD ; +/- zoom)
	if Input.is_physical_key_pressed(KEY_LEFT) or Input.is_physical_key_pressed(KEY_A):
		dy += 1.0
	if Input.is_physical_key_pressed(KEY_RIGHT) or Input.is_physical_key_pressed(KEY_D):
		dy -= 1.0
	if Input.is_physical_key_pressed(KEY_UP) or Input.is_physical_key_pressed(KEY_W):
		dp += 1.0
	if Input.is_physical_key_pressed(KEY_DOWN) or Input.is_physical_key_pressed(KEY_S):
		dp -= 1.0
	if Input.is_physical_key_pressed(KEY_EQUAL) or Input.is_physical_key_pressed(KEY_KP_ADD):
		dz -= 1.0
	if Input.is_physical_key_pressed(KEY_MINUS) or Input.is_physical_key_pressed(KEY_KP_SUBTRACT):
		dz += 1.0

	if dy == 0.0 and dp == 0.0 and dz == 0.0:
		return
	yaw += dy * ROT_SPEED * delta
	pitch = clampf(pitch + dp * ROT_SPEED * delta, -1.45, 1.45)
	if dz != 0.0:
		distance = clampf(distance * (1.0 + dz * ZOOM_RATE * delta), near_d * 2.0, far_d * 0.5)
	_update_cam()


func _unhandled_input(e: InputEvent) -> void:
	if e is InputEventMouseButton and e.pressed:
		match e.button_index:
			MOUSE_BUTTON_WHEEL_UP:
				_zoom_by(WHEEL_STEP)        # scroll suave (antes era agressivo)
			MOUSE_BUTTON_WHEEL_DOWN:
				_zoom_by(1.0 / WHEEL_STEP)
			MOUSE_BUTTON_LEFT:
				_pick(e.position)           # clicar centraliza (sem arrastar)


## Clica numa estrutura → centraliza a câmera nela e mostra o nome.
func _pick(pos: Vector2) -> void:
	var space := get_world_3d().direct_space_state
	var from := cam.project_ray_origin(pos)
	var to := from + cam.project_ray_normal(pos) * far_d
	var q := PhysicsRayQueryParameters3D.create(from, to)
	var hit := space.intersect_ray(q)
	if not hit or not hit.has("collider"):
		return
	var body: Object = hit["collider"]
	var mi: Node = body.get_parent()
	if mi is MeshInstance3D and mi.mesh:
		var ab: AABB = mi.global_transform * mi.mesh.get_aabb()
		target = ab.get_center()
		distance = clampf(ab.size.length() * 1.7, near_d * 4.0, far_d * 0.5)
		_update_cam()
	if body.has_meta("part_label"):
		label_pick.text = "> " + str(body.get_meta("part_label"))
	# Drill-down multiescala: clicar no encéfalo abre o viewer de núcleos (/neuro).
	if mi is MeshInstance3D and mi.name == "brain":
		_drill_to_nuclei()


## Do encéfalo (macro) → viewer de núcleos subcorticais (Neuroglancer em /neuro).
## No export web navega o browser; no desktop só registra (sem navegação).
func _drill_to_nuclei() -> void:
	label_pick.text = "> Encéfalo — abrindo núcleos…"
	if OS.has_feature("web"):
		JavaScriptBridge.eval("window.location.href = '/neuro'", true)
	else:
		print("[neuro] drill-down → /neuro (núcleos) — disponível no export web")


func _reset_view() -> void:
	target = home_target
	distance = home_distance
	yaw = 0.7
	pitch = 0.15
	_update_cam()


# ---- UI: controles na tela (zoom/girar/inclinar) + transparência + atalhos ----

func _hold_button(text: String, axis: String, dir: float) -> Button:
	var b := Button.new()
	b.text = text
	b.focus_mode = Control.FOCUS_NONE
	b.custom_minimum_size = Vector2(40, 34)
	b.button_down.connect(func() -> void: hold[axis] = dir)
	b.button_up.connect(func() -> void: hold[axis] = 0.0)
	return b


func _build_ui() -> void:
	var layer := CanvasLayer.new()
	add_child(layer)

	var title := Label.new()
	title.text = "Anatomia 3D — SNC no corpo"
	title.position = Vector2(16, 12)
	layer.add_child(title)

	label_pick = Label.new()
	label_pick.text = "> clique numa estrutura para centralizar"
	label_pick.position = Vector2(16, 36)
	layer.add_child(label_pick)

	# Transparência do corpo
	var slabel := Label.new()
	slabel.text = "Transparência do corpo"
	slabel.position = Vector2(16, 66)
	layer.add_child(slabel)
	var slider := HSlider.new()
	slider.min_value = 0.0
	slider.max_value = 1.0
	slider.step = 0.01
	slider.value = body_mat.albedo_color.a if body_mat else 0.16
	slider.custom_minimum_size = Vector2(200, 0)
	slider.position = Vector2(16, 88)
	slider.focus_mode = Control.FOCUS_NONE
	slider.value_changed.connect(func(v: float) -> void:
		if body_mat:
			body_mat.albedo_color = Color(body_mat.albedo_color, v))
	layer.add_child(slider)

	# Painel de controles (canto inferior esquerdo) — "in-game map"
	var panel := VBoxContainer.new()
	panel.position = Vector2(16, 140)
	panel.add_theme_constant_override("separation", 6)
	layer.add_child(panel)

	var row_rot := HBoxContainer.new()
	row_rot.add_child(_make_label("Girar/Inclinar"))
	panel.add_child(row_rot)
	var grid := GridContainer.new()
	grid.columns = 3
	grid.add_child(_spacer())
	grid.add_child(_hold_button("^", "pitch", 1.0))   # inclinar p/ cima
	grid.add_child(_spacer())
	grid.add_child(_hold_button("<", "yaw", 1.0))      # girar esquerda
	grid.add_child(_reset_btn())
	grid.add_child(_hold_button(">", "yaw", -1.0))     # girar direita
	grid.add_child(_spacer())
	grid.add_child(_hold_button("v", "pitch", -1.0))   # inclinar p/ baixo
	grid.add_child(_spacer())
	panel.add_child(grid)

	var row_zoom := HBoxContainer.new()
	row_zoom.add_theme_constant_override("separation", 6)
	row_zoom.add_child(_make_label("Zoom"))
	row_zoom.add_child(_hold_button("+", "zoom", -1.0))
	row_zoom.add_child(_hold_button("-", "zoom", 1.0))
	panel.add_child(row_zoom)

	var help := Label.new()
	help.text = "Atalhos: setas/WASD giram·inclinam · +/- zoom · clique = centraliza"
	help.add_theme_font_size_override("font_size", 12)
	panel.add_child(help)


func _make_label(t: String) -> Label:
	var l := Label.new()
	l.text = t
	return l


func _spacer() -> Control:
	var c := Control.new()
	c.custom_minimum_size = Vector2(40, 34)
	return c


func _reset_btn() -> Button:
	var b := Button.new()
	b.text = "R"  # reset/home
	b.focus_mode = Control.FOCUS_NONE
	b.custom_minimum_size = Vector2(40, 34)
	b.pressed.connect(_reset_view)
	return b


# ---- Captura headless (CAPTURE=1): frames p/ verificação ----

func _capture_sequence() -> void:
	await _shot("a-default")
	yaw += 1.0
	pitch = 0.35
	_update_cam()
	await _shot("b-rotated")
	if body_mat:
		body_mat.albedo_color = Color(body_mat.albedo_color, 0.0)
	await _shot("c-cns-only")
	get_tree().quit(0)


func _shot(name: String) -> void:
	for _i in range(3):
		await get_tree().process_frame
	await RenderingServer.frame_post_draw
	var img := get_viewport().get_texture().get_image()
	img.save_png("/tmp/anat3d-%s.png" % name)
	print("CAPTURED %s" % name)
