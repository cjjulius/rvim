(* OCaml sample for rvim syntax highlighting. *)
let distance (x1, y1) (x2, y2) =
  let dx = x1 -. x2 in
  let dy = y1 -. y2 in
  sqrt ((dx *. dx) +. (dy *. dy))

let () =
  let points = [ (0., 0.); (3., 4.) ] in
  List.iter
    (fun p -> Printf.printf "distance: %.2f\n" (distance p (0., 0.)))
    points
