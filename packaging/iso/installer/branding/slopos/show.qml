/* SLOPOS-I installer slideshow: one plain slide, no timer, so nothing
 * wakes up while the install runs.
 */
import QtQuick 2.0;
import calamares.slideshow 1.0;

Presentation
{
    id: presentation

    Slide {
        Rectangle {
            anchors.fill: parent
            color: "#1d1f22"
        }
        Text {
            anchors.centerIn: parent
            text: "SLOPOS-I"
            color: "#ffffff"
            font.pointSize: 24
        }
    }

    function onActivate() {
        presentation.currentSlide = 0;
    }
}
