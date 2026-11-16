# Engine

## Engine conventions

Cause I know I'll forget if I don't write it down.

**Coordinates** go:

- X — left to right
- Y — top to bottom
- Z — higher values are closer to camera (drawn on top)

**Coordinate spaces** are as follows:

- **object space** — local space of the object
- **world space** — global space of the scene
- **camera space** — space of the camera
    - camera is at origin, looking towards +Z
- **clip space** — pre-divide clip space (`[x, y, z, w]`)
- **NDC space** — normalized device coordinates, post-divide clip space (`[x/w, y/w, z/w, 1]`)
    - X and Y are in range [-1, 1]
    - Z is in range [0, 1]
- **target space** — pixel coordinates on the final render output framebuffer
    - origin is top-left corner
    - X increases to the right, Y increases downwards
- **screen space** — pixel coordinates on the screen, which may be different from target space if rendering to an
  offscreen framebuffer

Due to the engine being 2D, clip space and NDC space might be the same.
Projection divide is performed by the GPU, so mostly it's the *clip space* used in the engine,
rather than NDC.

**Matrices** are named as such:

| OpenGL            | Engine             |
|-------------------|--------------------|
| Model matrix      | `object_to_world`  |
| View matrix       | `world_to_camera`  |
| Projection matrix | `camera_to_clip`   |
| MV                | `object_to_camera` |
| VP                | `world_to_clip`    |
| MVP               | `object_to_clip`   |

and so on.

The convention is `<from>_to_<to>`. Names like *model matrix*, etc., should be avoided
in order to avoid confusion.

Column-major matrices are used, so for example:

```rs
object_to_clip = camera_to_clip * world_to_camera * object_to_world
```

The used names might be inconsistent for now, they should be updated to follow the convention eventually.
