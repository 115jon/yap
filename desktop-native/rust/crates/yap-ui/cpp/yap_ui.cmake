# The shared C++ behind Yap.Ui, for both apps' CMake projects:
#
#   include(<this folder>/yap_ui.cmake)
#   target_link_libraries(<app> PRIVATE yap_ui_cpp)
#
# An INTERFACE library: the sources compile inside each app's target, with
# its flags (the desktop's /GL and LTCG, the Android toolchain) and its
# AUTOMOC. Needs Qt's Gui, Qml, Quick and QuickPrivate found first
# (QSGPlainTexture, cpp/animatedpicture.cpp). The decoder it calls,
# yap_anim_*, is the yap-ui crate's (src/anim.rs).
add_library(yap_ui_cpp INTERFACE)
target_sources(yap_ui_cpp INTERFACE
  ${CMAKE_CURRENT_LIST_DIR}/yap_ui.cpp ${CMAKE_CURRENT_LIST_DIR}/yap_ui.h
  ${CMAKE_CURRENT_LIST_DIR}/animatedpicture.cpp ${CMAKE_CURRENT_LIST_DIR}/animatedpicture.h
  ${CMAKE_CURRENT_LIST_DIR}/ditherprovider.cpp ${CMAKE_CURRENT_LIST_DIR}/ditherprovider.h
  ${CMAKE_CURRENT_LIST_DIR}/iconprovider.cpp ${CMAKE_CURRENT_LIST_DIR}/iconprovider.h)
target_include_directories(yap_ui_cpp INTERFACE ${CMAKE_CURRENT_LIST_DIR})
target_link_libraries(yap_ui_cpp INTERFACE Qt::Gui Qt::Qml Qt::Quick Qt::QuickPrivate)
