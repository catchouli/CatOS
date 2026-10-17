// Stand-in for GLEW, which Live2D's framework includes on desktop Linux. The
// menu's GL context comes from EGL, and libOpenGL exports every GL function
// directly, so plain prototypes are all it needs, plus the stand-ins in
// glcompat.cpp that make it work on a core-profile context.
#pragma once
#define GL_GLEXT_PROTOTYPES 1
#include <GL/gl.h>
#include <GL/glext.h>

extern "C" {
void meownix_glVertexAttribPointer(GLuint index, GLint size, GLenum type, GLboolean normalized, GLsizei stride,
                                   const void* pointer);
void meownix_glDrawElements(GLenum mode, GLsizei count, GLenum type, const void* indices);
GLint meownix_gl_begin();
void meownix_gl_end(GLint previous);
}
#define glVertexAttribPointer meownix_glVertexAttribPointer
#define glDrawElements meownix_glDrawElements
