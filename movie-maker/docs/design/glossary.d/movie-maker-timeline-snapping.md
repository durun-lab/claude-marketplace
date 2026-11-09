## 再生と Timeline

| 用語 | 定義 | 識別子 |
|---|---|---|
| スナップの目標 | ドラッグ中の端を吸い付ける時刻と、その種類（再生位置、レイヤーの端、カットの端、キーフレーム、マーカー、コンポジションの始まりと終わり）。同じ近さなら、この順の先のものを採る。動かしている端そのものは目標から外す | `SnapTarget`、`SnapKind`、`snap::targets` |
| 吸い付いた先の文 | 何に吸い付いたかを出す 1 行（Snapped to Playhead、Snapped to Layer Edge、Snapped to Clip Edge、Snapped to Keyframe、Snapped to Marker、Snapped to Start、Snapped to End）。ステータスバーが出す | `SnapTarget::message` |
| 反転（スナップの） | ドラッグ中にスナップの入/切のキー（N）を押している間だけ、入/切を逆にすること。離すかドラッグが終わると戻り、入/切そのものは変えない（FCP の N と同じ） | `PointerEvent::ToggleSnapping`、`PointerEvent::SnapKeyUp`、`keys::release` |
| スナップの表示 | 吸い付いたときだけ出す、太さ 2 で時刻の列の全高のガイド線と、目盛りの帯の下端の下向きの三角、行の領域の下端の上向きの三角。吸い付いた先が帯の端なら、その端のつまみの幅をガイド線の色で塗る。端が目標に届かない（素材の端や 1 フレームの長さで止まった）ときは出さない | `PaintState::snap`、`paint/snap.rs` |
| 形のある図形の切り方 | ひし形と三角（`QuadShape`）は切ると形が変わるので、外接する矩形が切る矩形に全部入るときだけ描く | `Canvas::push_shape` |
| ドラッグ中のツールチップ | 編集のドラッグの間、ポインターの右（入らなければ左）に出す、動かした量の矩形。量はポインターの向き（右が正）で、1 秒より短ければフレーム数（+12f）、1 秒からは時:分:秒:フレーム（−00:00:01:05）。矩形に重なるほかの文字は矩形の手前で切り、再生位置の線とスナップの表示は矩形の外だけに描く | `DragTip`、`drag_tooltip`、`drag_amount`、`clear_tooltip_area` |
