// Bridge between the menu and Live2D's Cubism framework: loads one model and
// draws it with OpenGL into the menu's own GL context. See avatar.rs.

#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <string>

#include <CubismDefaultParameterId.hpp>
#include <CubismFramework.hpp>
#include <CubismModelSettingJson.hpp>
#include <Effect/CubismBreath.hpp>
#include <Effect/CubismEyeBlink.hpp>
#include <Effect/CubismLook.hpp>
#include <Id/CubismIdManager.hpp>
#include <Math/CubismMatrix44.hpp>
#include <Math/CubismModelMatrix.hpp>
#include <Model/CubismUserModel.hpp>
#include <Motion/CubismBreathUpdater.hpp>
#include <Motion/CubismEyeBlinkUpdater.hpp>
#include <Motion/CubismLookUpdater.hpp>
#include <Motion/CubismMotion.hpp>
#include <Motion/CubismPhysicsUpdater.hpp>
#include <Motion/CubismPoseUpdater.hpp>
#include <Rendering/OpenGL/CubismOffscreenManager_OpenGLES2.hpp>
#include <Rendering/OpenGL/CubismRenderer_OpenGLES2.hpp>

using namespace Live2D::Cubism::Framework;
using namespace Live2D::Cubism::Framework::DefaultParameterId;
using Live2D::Cubism::Framework::Rendering::CubismOffscreenManager_OpenGLES2;
using Live2D::Cubism::Framework::Rendering::CubismRenderer_OpenGLES2;

namespace {

// The framework asks for its shaders as "FrameworkShaders/<name>"; they're
// installed under this folder.
std::string g_shader_dir;

csmByte* load_file(const std::string path, csmSizeInt* size)
{
    std::string full = path;
    if (path.rfind("FrameworkShaders/", 0) == 0) {
        full = g_shader_dir + "/" + path.substr(17);
    }
    std::ifstream f(full, std::ios::binary | std::ios::ate);
    if (!f) {
        std::fprintf(stderr, "meownix-menu: avatar: can't read %s\n", full.c_str());
        *size = 0;
        return nullptr;
    }
    *size = static_cast<csmSizeInt>(f.tellg());
    auto* buf = static_cast<csmByte*>(std::malloc(*size ? *size : 1));
    f.seekg(0);
    f.read(reinterpret_cast<char*>(buf), *size);
    return buf;
}

void release_file(csmByte* bytes) { std::free(bytes); }

void log_message(const char* msg) { std::fprintf(stderr, "meownix-menu: live2d: %s", msg); }

class Allocator : public ICubismAllocator {
    void* Allocate(const csmSizeType size) override { return std::malloc(size); }
    void Deallocate(void* p) override { std::free(p); }
    void* AllocateAligned(const csmSizeType size, const csmUint32 alignment) override
    {
        void* p = nullptr;
        return posix_memalign(&p, alignment < sizeof(void*) ? sizeof(void*) : alignment, size) == 0 ? p : nullptr;
    }
    void DeallocateAligned(void* p) override { std::free(p); }
};

Allocator g_allocator;
CubismFramework::Option g_option;

const csmInt32 PriorityIdle = 1;

class Avatar : public CubismUserModel {
public:
    ~Avatar() override
    {
        for (auto it = _motions.Begin(); it != _motions.End(); ++it) {
            ACubismMotion::Delete(it->Second);
        }
        delete _setting;
    }

    bool Load(const std::string& model_json)
    {
        const size_t slash = model_json.rfind('/');
        _dir = slash == std::string::npos ? "" : model_json.substr(0, slash + 1);

        csmSizeInt size;
        csmByte* buf = load_file(model_json, &size);
        if (!buf) return false;
        _setting = new CubismModelSettingJson(buf, size);
        release_file(buf);

        if (!LoadFile(_setting->GetModelFileName(), [&](csmByte* b, csmSizeInt n) { LoadModel(b, n); }) || !_model) {
            return false;
        }
        LoadFile(_setting->GetPhysicsFileName(), [&](csmByte* b, csmSizeInt n) { LoadPhysics(b, n); });
        LoadFile(_setting->GetPoseFileName(), [&](csmByte* b, csmSizeInt n) { LoadPose(b, n); });
        if (_physics) _updateScheduler.AddUpdatableList(CSM_NEW CubismPhysicsUpdater(*_physics));
        if (_pose) _updateScheduler.AddUpdatableList(CSM_NEW CubismPoseUpdater(*_pose));

        if (_setting->GetEyeBlinkParameterCount() > 0) {
            _eyeBlink = CubismEyeBlink::Create(_setting);
            _updateScheduler.AddUpdatableList(CSM_NEW CubismEyeBlinkUpdater(_motionUpdated, *_eyeBlink));
        }
        for (csmInt32 i = 0; i < _setting->GetEyeBlinkParameterCount(); i++) {
            _eyeBlinkIds.PushBack(_setting->GetEyeBlinkParameterId(i));
        }
        for (csmInt32 i = 0; i < _setting->GetLipSyncParameterCount(); i++) {
            _lipSyncIds.PushBack(_setting->GetLipSyncParameterId(i));
        }

        // Breathing and looking around, with the same strengths as Live2D's samples.
        CubismIdManager* ids = CubismFramework::GetIdManager();
        _breath = CubismBreath::Create();
        csmVector<CubismBreath::BreathParameterData> breath;
        breath.PushBack(CubismBreath::BreathParameterData(ids->GetId(ParamAngleX), 0.0f, 15.0f, 6.5345f, 0.5f));
        breath.PushBack(CubismBreath::BreathParameterData(ids->GetId(ParamAngleY), 0.0f, 8.0f, 3.5345f, 0.5f));
        breath.PushBack(CubismBreath::BreathParameterData(ids->GetId(ParamAngleZ), 0.0f, 10.0f, 5.5345f, 0.5f));
        breath.PushBack(CubismBreath::BreathParameterData(ids->GetId(ParamBodyAngleX), 0.0f, 4.0f, 15.5345f, 0.5f));
        breath.PushBack(CubismBreath::BreathParameterData(ids->GetId(ParamBreath), 0.5f, 0.5f, 3.2345f, 0.5f));
        _breath->SetParameters(breath);
        _updateScheduler.AddUpdatableList(CSM_NEW CubismBreathUpdater(*_breath));

        _look = CubismLook::Create();
        csmVector<CubismLook::LookParameterData> look;
        look.PushBack(CubismLook::LookParameterData(ids->GetId(ParamAngleX), 30.0f));
        look.PushBack(CubismLook::LookParameterData(ids->GetId(ParamAngleY), 0.0f, 30.0f));
        look.PushBack(CubismLook::LookParameterData(ids->GetId(ParamAngleZ), 0.0f, 0.0f, -30.0f));
        look.PushBack(CubismLook::LookParameterData(ids->GetId(ParamBodyAngleX), 10.0f));
        look.PushBack(CubismLook::LookParameterData(ids->GetId(ParamEyeBallX), 1.0f));
        look.PushBack(CubismLook::LookParameterData(ids->GetId(ParamEyeBallY), 0.0f, 1.0f));
        _look->SetParameters(look);
        _updateScheduler.AddUpdatableList(CSM_NEW CubismLookUpdater(*_look, *_dragManager));
        _updateScheduler.SortUpdatableList();

        csmMap<csmString, csmFloat32> layout;
        _setting->GetLayoutMap(layout);
        _modelMatrix->SetupFromLayout(layout);
        _model->SaveParameters();

        // Only the idle motions are used: the menu plays them on a loop.
        for (csmInt32 i = 0; i < _setting->GetMotionCount("Idle"); i++) {
            LoadFile(_setting->GetMotionFileName("Idle", i), [&](csmByte* b, csmSizeInt n) {
                auto* m = static_cast<CubismMotion*>(LoadMotion(b, n, nullptr, nullptr, nullptr, _setting, "Idle", i));
                if (m) {
                    m->SetEffectIds(_eyeBlinkIds, _lipSyncIds);
                    _motions[i] = m;
                }
            });
        }
        return true;
    }

    /// Loads the textures (decoded by the menu) and sets up rendering. Needs the GL context.
    void CreateRenderer(csmUint32 width, csmUint32 height)
    {
        CubismUserModel::CreateRenderer(width, height);
        // The menu uploads textures with premultiplied alpha.
        GetRenderer<CubismRenderer_OpenGLES2>()->IsPremultipliedAlpha(true);
    }

    csmInt32 TextureCount() const { return _setting->GetTextureCount(); }

    std::string TexturePath(csmInt32 i) const { return _dir + _setting->GetTextureFileName(i); }

    void Update(csmFloat32 dt, csmFloat32 look_x, csmFloat32 look_y)
    {
        SetDragging(look_x, look_y); // eased towards by the look updater

        _motionUpdated = false;
        _model->LoadParameters();
        if (_motionManager->IsFinished()) {
            if (_motions.GetSize() > 0) {
                ACubismMotion* m = _motions[std::rand() % _motions.GetSize()];
                _motionManager->StartMotionPriority(m, false, PriorityIdle);
            }
        } else {
            _motionUpdated = _motionManager->UpdateMotion(_model, dt);
        }
        _model->SaveParameters();
        _updateScheduler.OnLateUpdate(_model, dt);
        _model->Update();
    }

    /// Draws into the current framebuffer. `projection` maps the model's own
    /// coordinates (about -1..1, the height of the model) to clip space.
    void Draw(CubismMatrix44& projection)
    {
        projection.MultiplyByMatrix(_modelMatrix);
        auto* r = GetRenderer<CubismRenderer_OpenGLES2>();
        r->SetMvpMatrix(&projection);
        r->DrawModel();
    }

    /// Model width / height, from the model's canvas.
    csmFloat32 Aspect() const { return _model->GetCanvasWidth() / _model->GetCanvasHeight(); }

private:
    template <class F>
    bool LoadFile(const csmChar* name, F use)
    {
        if (!name || !*name) return false;
        csmSizeInt size;
        csmByte* buf = load_file(_dir + name, &size);
        if (!buf) return false;
        use(buf, size);
        release_file(buf);
        return true;
    }

    std::string _dir;
    ICubismModelSetting* _setting = nullptr;
    csmBool _motionUpdated = false;
    csmVector<CubismIdHandle> _eyeBlinkIds, _lipSyncIds;
    csmMap<csmInt32, ACubismMotion*> _motions;
};

} // namespace

extern "C" {

/// Starts the framework and loads the model's files (no GL needed yet).
/// Returns null if the model can't be loaded.
void* meownix_avatar_load(const char* model_json, const char* shader_dir)
{
    if (!CubismFramework::IsStarted()) {
        g_shader_dir = shader_dir;
        g_option.LogFunction = log_message;
        g_option.LoggingLevel = CubismFramework::Option::LogLevel_Warning;
        g_option.LoadFileFunction = load_file;
        g_option.ReleaseBytesFunction = release_file;
        CubismFramework::StartUp(&g_allocator, &g_option);
        CubismFramework::Initialize();
    }
    auto* a = new Avatar();
    if (!a->Load(model_json)) {
        delete a;
        return nullptr;
    }
    return a;
}

int meownix_avatar_texture_count(void* a) { return static_cast<Avatar*>(a)->TextureCount(); }

/// Writes texture `i`'s file path into `buf` (NUL-terminated, truncated to `len`).
void meownix_avatar_texture_path(void* a, int i, char* buf, int len)
{
    std::snprintf(buf, len, "%s", static_cast<Avatar*>(a)->TexturePath(i).c_str());
}

float meownix_avatar_aspect(void* a) { return static_cast<Avatar*>(a)->Aspect(); }

/// Creates the GL renderer and binds the textures the menu uploaded. Call with
/// the menu's GL context current.
void meownix_avatar_init_gl(void* a, const unsigned* textures, int count, int width, int height)
{
    auto* av = static_cast<Avatar*>(a);
    av->CreateRenderer(width, height);
    for (int i = 0; i < count; i++) {
        av->GetRenderer<CubismRenderer_OpenGLES2>()->BindTexture(i, textures[i]);
    }
}

/// Advances the animation. `look_x`/`look_y` (-1..1, +y up) is where she looks.
void meownix_avatar_update(void* a, float dt, float look_x, float look_y)
{
    static_cast<Avatar*>(a)->Update(dt, look_x, look_y);
}

/// Draws her with the model's centre at (`x`, `y`) in clip space, `height` tall
/// (also in clip space), into a `width_px` x `height_px` framebuffer.
void meownix_avatar_draw(void* a, float x, float y, float height, int width_px, int height_px)
{
    auto* av = static_cast<Avatar*>(a);
    av->SetRenderTargetSize(width_px, height_px);

    CubismMatrix44 projection;
    // The model matrix makes her 2 units tall; keep her proportions on a wide screen.
    projection.Scale(height / 2.0f * height_px / width_px, height / 2.0f);
    projection.Translate(x, y);

    const GLint vao = meownix_gl_begin();
    CubismOffscreenManager_OpenGLES2::GetInstance()->BeginFrameProcess();
    av->Draw(projection);
    CubismOffscreenManager_OpenGLES2::GetInstance()->EndFrameProcess();
    CubismOffscreenManager_OpenGLES2::GetInstance()->ReleaseStaleRenderTextures();
    meownix_gl_end(vao);
}

void meownix_avatar_free(void* a) { delete static_cast<Avatar*>(a); }

} // extern "C"
