// Lets Live2D's OpenGL renderer run on a core-profile context (what the menu
// gets from Mesa). The renderer passes vertex and index data straight from
// memory (client-side arrays), which core profile doesn't allow, so these
// stand-ins copy that data into buffers just before each draw. include/GL/glew.h
// points the framework's calls here; this file sees the real functions.

#define GL_GLEXT_PROTOTYPES 1
#include <GL/gl.h>
#include <GL/glext.h>

#include <algorithm>
#include <cstdint>

namespace {

struct ClientArray {
    const void* pointer = nullptr;
    GLint size = 0;
    GLenum type = 0;
    GLboolean normalized = GL_FALSE;
    GLsizei stride = 0;
};

const GLuint MaxAttribs = 16;
ClientArray g_arrays[MaxAttribs];
GLuint g_vertex_buffers[MaxAttribs];
GLuint g_index_buffer;
GLuint g_vao;

GLsizei type_size(GLenum type)
{
    switch (type) {
    case GL_UNSIGNED_BYTE: case GL_BYTE: return 1;
    case GL_UNSIGNED_SHORT: case GL_SHORT: return 2;
    default: return 4;
    }
}

GLint bound(GLenum which)
{
    GLint b = 0;
    glGetIntegerv(which, &b);
    return b;
}

} // namespace

extern "C" {

void catos_glVertexAttribPointer(GLuint index, GLint size, GLenum type, GLboolean normalized, GLsizei stride,
                                   const void* pointer)
{
    if (bound(GL_ARRAY_BUFFER_BINDING) != 0 || index >= MaxAttribs) {
        glVertexAttribPointer(index, size, type, normalized, stride, pointer);
        return;
    }
    g_arrays[index] = {pointer, size, type, normalized, stride};
}

void catos_glDrawElements(GLenum mode, GLsizei count, GLenum type, const void* indices)
{
    if (bound(GL_ELEMENT_ARRAY_BUFFER_BINDING) != 0) {
        glDrawElements(mode, count, type, indices);
        return;
    }
    if (!g_index_buffer) {
        glGenBuffers(1, &g_index_buffer);
        glGenBuffers(MaxAttribs, g_vertex_buffers);
    }

    // How many vertices the indices refer to, so we know how much to copy.
    GLuint max_index = 0;
    for (GLsizei i = 0; i < count; i++) {
        GLuint v = type == GL_UNSIGNED_SHORT  ? static_cast<const uint16_t*>(indices)[i]
                   : type == GL_UNSIGNED_BYTE ? static_cast<const uint8_t*>(indices)[i]
                                              : static_cast<const uint32_t*>(indices)[i];
        max_index = std::max(max_index, v);
    }

    for (GLuint a = 0; a < MaxAttribs; a++) {
        const ClientArray& c = g_arrays[a];
        if (!c.pointer) continue;
        const GLsizei stride = c.stride ? c.stride : c.size * type_size(c.type);
        glBindBuffer(GL_ARRAY_BUFFER, g_vertex_buffers[a]);
        glBufferData(GL_ARRAY_BUFFER, stride * (max_index + 1), c.pointer, GL_STREAM_DRAW);
        glVertexAttribPointer(a, c.size, c.type, c.normalized, c.stride, nullptr);
    }
    glBindBuffer(GL_ARRAY_BUFFER, 0);

    glBindBuffer(GL_ELEMENT_ARRAY_BUFFER, g_index_buffer);
    glBufferData(GL_ELEMENT_ARRAY_BUFFER, count * type_size(type), indices, GL_STREAM_DRAW);
    glDrawElements(mode, count, type, nullptr);
    glBindBuffer(GL_ELEMENT_ARRAY_BUFFER, 0);

    // The renderer sets fresh pointers before every draw.
    std::fill(g_arrays, g_arrays + MaxAttribs, ClientArray{});
}

/// Core profile needs a vertex array object bound to draw at all. Binds ours
/// and returns the one that was bound, for catos_gl_end.
GLint catos_gl_begin()
{
    if (!g_vao) glGenVertexArrays(1, &g_vao);
    GLint previous = bound(GL_VERTEX_ARRAY_BINDING);
    glBindVertexArray(g_vao);
    return previous;
}

void catos_gl_end(GLint previous) { glBindVertexArray(previous); }

} // extern "C"
